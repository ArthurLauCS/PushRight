use crate::{err, refs, Repo, Result};
use gix::ObjectId;
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

const NONE: u32 = u32::MAX;
const CHECKPOINT: usize = 1024;

/// 提交图骨架：只有 id 和父子关系，作者/标题在 `rows` 里按窗口现取。
pub struct Graph {
    ids: Vec<ObjectId>,
    index: HashMap<ObjectId, u32>,
    /// 第 r 行的父行号是 parents[parent_start[r]..parent_start[r + 1]]
    parents: Vec<u32>,
    parent_start: Vec<u32>,
    /// 每 CHECKPOINT 行存一份泳道状态，`rows` 从最近的快照重放到窗口起点
    checkpoints: Vec<Vec<u32>>,
}

#[derive(Serialize, Default)]
pub struct Row {
    pub id: String,
    pub lane: u32,
    /// 从上方汇入本提交的泳道
    pub incoming: Vec<u32>,
    /// 穿过本行、与本提交无关的泳道
    pub through: Vec<u32>,
    /// 从本提交向下连到各个父提交的泳道
    pub out: Vec<u32>,
    pub author: String,
    pub author_email: String,
    pub time: i64,
    pub subject: String,
}

impl Graph {
    /// 从所有引用出发加载最近的 `limit` 个提交，按「子在父之前、其余按时间」排序。
    pub fn load(repo: &Repo, limit: usize) -> Result<Graph> {
        Self::load_scope(repo, limit, "all")
    }

    pub fn load_scope(repo: &Repo, limit: usize, scope: &str) -> Result<Graph> {
        Self::load_view(repo, limit, scope, false)
    }

    /// `first_parent`：只沿每个提交的第一个父提交走，合并进来的那一串提交不展开（同 `git log --first-parent`）。
    pub fn load_view(repo: &Repo, limit: usize, scope: &str, first_parent: bool) -> Result<Graph> {
        let gix = repo.gix();
        let (tips, hidden) = refs::graph_tips(repo, scope)?;
        let nodes = if tips.is_empty() {
            Vec::new()
        } else {
            let walk = gix.rev_walk(tips).with_hidden(hidden);
            if first_parent { walk.first_parent_only() } else { walk }
                .sorting(gix::revision::walk::Sorting::ByCommitTime(Default::default()))
                .all()
                .map_err(err)?
                .take(limit)
                .map(|info| info.map(|i| i.detach()).map_err(err))
                .collect::<Result<Vec<_>>>()?
        };

        // 按时间遍历在时钟不准的提交上可能把父排在子前面，用 Kahn 排序纠正
        let walk_index: HashMap<_, _> = nodes.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
        let mut indegree = vec![0u32; nodes.len()];
        for c in &nodes {
            for p in &c.parent_ids {
                if let Some(&i) = walk_index.get(p) {
                    indegree[i] += 1;
                }
            }
        }
        // 只看主线时遍历不带提交时间，自己从提交里读；否则同一时刻可选的几条线会按遍历顺序而不是时间排
        let times: Vec<i64> = nodes
            .iter()
            .map(|c| c.commit_time.or_else(|| Some(gix.find_commit(c.id).ok()?.time().ok()?.seconds)).unwrap_or(0))
            .collect();
        let key = |i: usize| (times[i], Reverse(i));
        let mut heap: BinaryHeap<_> = (0..nodes.len()).filter(|&i| indegree[i] == 0).map(key).collect();
        let mut ids = Vec::with_capacity(nodes.len());
        while let Some((_, Reverse(i))) = heap.pop() {
            ids.push(nodes[i].id);
            for p in &nodes[i].parent_ids {
                if let Some(&pi) = walk_index.get(p) {
                    indegree[pi] -= 1;
                    if indegree[pi] == 0 {
                        heap.push(key(pi));
                    }
                }
            }
        }

        let index: HashMap<_, _> = ids.iter().enumerate().map(|(r, id)| (*id, r as u32)).collect();
        let mut parents = Vec::with_capacity(ids.len());
        let mut parent_start = Vec::with_capacity(ids.len() + 1);
        for id in &ids {
            parent_start.push(parents.len() as u32);
            // ponytail: 被 limit 截断在外的父提交直接丢弃，边界上的提交看起来像根提交；需要时给它们画一条到底的线
            parents.extend(nodes[walk_index[id]].parent_ids.iter().filter_map(|p| index.get(p)));
        }
        parent_start.push(parents.len() as u32);

        let mut g = Graph { ids, index, parents, parent_start, checkpoints: Vec::new() };
        let mut active = Vec::new();
        for r in 0..g.ids.len() {
            if r % CHECKPOINT == 0 {
                g.checkpoints.push(active.clone());
            }
            step(&mut active, r as u32, g.parents_of(r), None);
        }
        Ok(g)
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn row_of(&self, id: &str) -> Option<u32> {
        self.index.get(&ObjectId::from_hex(id.as_bytes()).ok()?).copied()
    }

    pub fn rows(&self, repo: &Repo, start: usize, count: usize) -> Result<Vec<Row>> {
        let end = (start + count).min(self.ids.len());
        if start >= end {
            return Ok(Vec::new());
        }
        let gix = repo.gix();
        let cp = start / CHECKPOINT;
        let mut active = self.checkpoints[cp].clone();
        for r in cp * CHECKPOINT..start {
            step(&mut active, r as u32, self.parents_of(r), None);
        }
        (start..end)
            .map(|r| {
                let mut row = Row { id: self.ids[r].to_string(), ..Default::default() };
                step(&mut active, r as u32, self.parents_of(r), Some(&mut row));
                let commit = gix.find_commit(self.ids[r]).map_err(err)?;
                let c = commit.decode().map_err(err)?;
                let author = c.author().map_err(err)?;
                row.author = author.name.to_string();
                row.author_email = author.email.to_string();
                row.time = c.committer().map_err(err)?.seconds();
                row.subject = c.message().title.to_string().trim_end().to_owned();
                Ok(row)
            })
            .collect()
    }

    fn parents_of(&self, r: usize) -> &[u32] {
        &self.parents[self.parent_start[r] as usize..self.parent_start[r + 1] as usize]
    }
}

/// 推进一行。`active[i]` 是泳道 i 正在等待的行号，NONE 为空闲。
// ponytail: 线性扫描泳道，O(行数 × 图宽)；内核级图宽（572）下 30 万行约 0.5 秒，成为瓶颈时换成 行号→泳道 的索引
fn step(active: &mut Vec<u32>, row: u32, parents: &[u32], mut draw: Option<&mut Row>) {
    let mut lane = NONE;
    for (i, slot) in active.iter_mut().enumerate() {
        if *slot == row {
            if lane == NONE {
                lane = i as u32;
            }
            if let Some(d) = &mut draw {
                d.incoming.push(i as u32);
            }
            *slot = NONE;
        } else if *slot != NONE {
            if let Some(d) = &mut draw {
                d.through.push(i as u32);
            }
        }
    }
    if lane == NONE {
        lane = free(active);
    }
    for (n, &p) in parents.iter().enumerate() {
        let l = if n == 0 {
            lane
        } else {
            active.iter().position(|&s| s == p).map(|i| i as u32).unwrap_or_else(|| free(active))
        };
        active[l as usize] = p;
        if let Some(d) = &mut draw {
            d.out.push(l);
        }
    }
    while active.last() == Some(&NONE) {
        active.pop();
    }
    if let Some(d) = draw {
        d.lane = lane;
    }
}

fn free(active: &mut Vec<u32>) -> u32 {
    active.iter().position(|&s| s == NONE).unwrap_or_else(|| {
        active.push(NONE);
        active.len() - 1
    }) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_opens_and_closes_a_lane() {
        // 行 0 合并 行 2、行 1，二者都基于 行 3；行 4 是另一个根
        let parents: [&[u32]; 5] = [&[2, 1], &[3], &[3], &[], &[]];
        let mut active = Vec::new();
        let rows: Vec<Row> = parents
            .iter()
            .enumerate()
            .map(|(r, p)| {
                let mut row = Row::default();
                step(&mut active, r as u32, p, Some(&mut row));
                row
            })
            .collect();
        assert_eq!(rows.iter().map(|r| r.lane).collect::<Vec<_>>(), [0, 1, 0, 0, 0]);
        assert_eq!(rows[0].out, [0, 1]);
        assert_eq!(rows[1].through, [0]);
        assert_eq!(rows[3].incoming, [0, 1]);
        assert!(active.is_empty());
    }
}
