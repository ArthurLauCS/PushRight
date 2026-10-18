//! 每个页签一个仓库会话。引擎调用都在后台线程跑，结果回到界面线程再改视图，界面不会因为 git 卡住。

use engine::graph::Graph;
use engine::Repo;
use gpui::{AppContext, Context, Task};
use std::sync::{Arc, RwLock};

/// 打开仓库时先加载这么多提交出首屏，完整历史在后台补齐
pub const FIRST_PAGE: usize = 2000;

pub struct Session {
    pub repo: Repo,
    graph: RwLock<Arc<Graph>>,
}

impl Session {
    pub fn open(path: &str) -> engine::Result<Session> {
        let repo = Repo::open(path.as_ref())?;
        let graph = Graph::load(&repo, FIRST_PAGE)?;
        Ok(Session { repo, graph: RwLock::new(Arc::new(graph)) })
    }

    pub fn path(&self) -> String {
        self.repo.path.display().to_string()
    }

    pub fn graph(&self) -> Arc<Graph> {
        self.graph.read().unwrap().clone()
    }

    pub fn set_graph(&self, graph: Graph) {
        *self.graph.write().unwrap() = Arc::new(graph);
    }
}

/// 在后台线程跑 `work`，跑完在界面线程调 `done`；视图已经关掉时丢弃结果
pub fn bg<V: 'static, T: Send + 'static>(
    cx: &mut Context<V>,
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(&mut V, T, &mut Context<V>) + 'static,
) -> Task<()> {
    cx.spawn(async move |this, cx| {
        let result = cx.background_spawn(async move { work() }).await;
        let _ = this.update(cx, |view, cx| done(view, result, cx));
    })
}
