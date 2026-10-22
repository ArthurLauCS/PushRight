//! 拉取请求（GitHub）/ 合并请求（GitLab）：左边列表，右边详情（说明、检查、审阅、提交、改动的文件和行评论）和操作。
//! 令牌先找凭据管理器里存过的，再找环境变量、gh、git 凭据，都用不了才请用户粘贴一个，验证通过后存进凭据管理器。
//! GitLab 上指定了审核者、自己既不是审核者也不是作者时，批准、评论、合并、关闭都先提醒一下，仍然可以做。
//! GitLab 新建时选指派人（默认自己）和审核者（默认上次指派的），之后可以改审核者；指派过的审核者按项目记在本地，排在选项前面。

use crate::credential;
use crate::detail::{DetailView, Source};
use crate::dialog::{Field, Spec, Values};
use crate::git::Session;
use crate::i18n::{error_text, lookup, t};
use crate::icons::Icon;
use crate::layout::{self, layout, Key};
use crate::repo_view::{Kind as RefKind, RepoView};
use crate::store;
use crate::theme::{alpha, palette, Palette, DISPLAY, FS_LG, FS_MD, FS_SM, LH_BODY, MONO, PROSE, R_LG};
use crate::time::fmt_time;
use crate::ui::btn;
use engine::forge::{self, Anchor, Commit, Forge, Kind, Member, NewRequest, Request, Spot, Status};
use engine::ops::Op;
use gpui::prelude::*;
use gpui::{div, px, uniform_list, AnyElement, App, Context, Entity, FontWeight, Hsla, UniformListScrollHandle, WeakEntity, Window};
use gpui_component::scroll::ScrollableElement;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    Open,
    Mine,
    Review,
    All,
}

pub struct Requests {
    session: Arc<Session>,
    repo: WeakEntity<RepoView>,
    forge: Option<Forge>,
    token: String,
    /// 令牌对应的用户名，「我创建的」「待我审阅」按它筛
    me: String,
    filter: Filter,
    list: Vec<Request>,
    scroll: UniformListScrollHandle,
    sel: Option<Request>,
    status: Option<Status>,
    commits: Vec<Commit>,
    /// 右边显示改动的文件，否则显示概览
    show_files: bool,
    files: Entity<DetailView>,
    /// 取 PR 的提交失败的原因：概览照常显示，文件页显示它
    files_error: String,
    error: String,
    connecting: bool,
    /// 拉列表、合并、批准、新建这类请求进行中
    busy: bool,
    /// 还没连上平台时点了「新建」：连上后接着弹
    pending: Option<String>,
    request: usize,
}

const ROW_H: f32 = 60.;

/// 列表和详情里的状态：打开的草稿单独算
fn state(r: &Request) -> &str {
    if r.state == "open" && r.draft { "draft" } else { &r.state }
}

fn state_color(state: &str, p: &Palette) -> Hsla {
    match state {
        "open" | "success" => p.green,
        "merged" => p.accent,
        "closed" | "failure" => p.red,
        "pending" => p.yellow,
        _ => p.muted,
    }
}

fn dot(color: Hsla) -> impl IntoElement {
    div().flex_none().size(px(8.)).rounded_full().bg(color)
}

/// 各项目指派过的审核者：键是 `主机/项目`，值是用户名，最近的在前
const REVIEWERS: &str = "reviewers";

fn recent(f: &Forge, cx: &App) -> Vec<String> {
    store::get::<HashMap<String, Vec<String>>>(cx, REVIEWERS).and_then(|mut m| m.remove(&format!("{}/{}", f.host, f.project))).unwrap_or_default()
}

/// 记下这次指派的审核者，下次默认选他；选「无」不记
fn remember(f: &Forge, members: &[Member], id: u64, cx: &mut App) {
    let Some(m) = members.iter().find(|m| m.id == id) else { return };
    let mut all = store::get::<HashMap<String, Vec<String>>>(cx, REVIEWERS).unwrap_or_default();
    let list = all.entry(format!("{}/{}", f.host, f.project)).or_default();
    list.retain(|n| *n != m.username);
    list.insert(0, m.username.clone());
    store::set(cx, REVIEWERS, all);
}

/// 用户名对应的选项值（用户 id），不是成员时是「无」
fn id_of(members: &[Member], name: &str) -> String {
    members.iter().find(|m| m.username == name).map_or("0".into(), |m| m.id.to_string())
}

/// 指派人、审核者的选项：「无」、指派过的审核者（最近的在前）、其余成员
fn people(members: &[Member], recent: &[String], cx: &App) -> Vec<(String, String)> {
    let mut sorted: Vec<&Member> = members.iter().collect();
    sorted.sort_by_key(|m| recent.iter().position(|n| *n == m.username).unwrap_or(usize::MAX));
    std::iter::once(("0".into(), t(cx).requests.nobody.into())).chain(sorted.into_iter().map(|m| (m.id.to_string(), format!("{} @{}", m.name, m.username)))).collect()
}

impl Requests {
    pub fn new(session: Arc<Session>, repo: WeakEntity<RepoView>, window: &mut Window, cx: &mut Context<Self>) -> Requests {
        let files = cx.new(|cx| DetailView::new(session.clone(), repo.clone(), window, cx));
        let me = cx.weak_entity();
        files.update(cx, |d, _| {
            d.comparison = true;
            d.on_comment = Some(Rc::new(move |a, window, cx| {
                let _ = me.update(cx, |this, cx| this.comment(a, String::new(), None, window, cx));
            }));
        });
        let mut this = Requests {
            session,
            repo,
            forge: None,
            token: String::new(),
            me: String::new(),
            filter: Filter::Open,
            list: Vec::new(),
            scroll: UniformListScrollHandle::new(),
            sel: None,
            status: None,
            commits: Vec::new(),
            show_files: false,
            files,
            files_error: String::new(),
            error: String::new(),
            connecting: false,
            busy: false,
            pending: None,
            request: 0,
        };
        this.connect(window, cx);
        this
    }

    /// 认出平台、找一个能用的令牌；有令牌才拉列表
    fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.connecting = true;
        self.error.clear();
        cx.notify();
        let session = self.session.clone();
        cx.spawn_in(window, async move |this, cx| {
            let found = cx
                .background_spawn(async move {
                    let f = forge::detect(&session.repo)?;
                    for token in credential::load(&f.host).into_iter().chain(forge::found_tokens(&session.repo, &f)) {
                        match forge::user(&f, &token) {
                            Ok(me) => return Ok((f, token, me)),
                            // 连不上服务器就不用一个个试了
                            Err(e) if e.starts_with("PR_FORGE_NETWORK") || e.starts_with("PR_FORGE_CURL") => return Err(e),
                            Err(_) => {}
                        }
                    }
                    Ok((f, String::new(), String::new()))
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.connecting = false;
                match found {
                    Ok((f, token, me)) => {
                        this.forge = Some(f);
                        this.signed_in(token, me, window, cx);
                    }
                    Err(e) => this.error = error_text(cx, &e),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn signed_in(&mut self, token: String, me: String, window: &mut Window, cx: &mut Context<Self>) {
        self.token = token;
        self.me = me;
        if self.token.is_empty() {
            return;
        }
        self.reload(cx);
        if let Some(branch) = self.pending.take() {
            self.create(branch, window, cx);
        }
    }

    /// 后台调一次平台接口；失败时把原因显示在页面上
    fn query<T: Send + 'static>(&mut self, work: impl FnOnce(&Forge, &str) -> engine::Result<T> + Send + 'static, done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static, cx: &mut Context<Self>) {
        let Some(f) = self.forge.clone() else { return };
        let token = self.token.clone();
        self.busy = true;
        self.error.clear();
        cx.notify();
        crate::git::bg(cx, move || work(&f, &token), move |this, r, cx| {
            this.busy = false;
            match r {
                Ok(v) => done(this, v, cx),
                Err(e) => this.error = error_text(cx, &e),
            }
            cx.notify();
        })
        .detach();
    }

    /// 用仓库页的对话框问一下，确认了再做 `f`
    fn ask(&mut self, spec: Spec, window: &mut Window, cx: &mut Context<Self>, f: impl FnOnce(&mut Self, Values, &mut Window, &mut Context<Self>) + 'static) {
        let Ok(rx) = self.repo.update(cx, |r, cx| r.ask(spec, window, cx)) else { return };
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Some(v)) = rx.await {
                let _ = this.update_in(cx, |this, window, cx| f(this, v, window, cx));
            }
        })
        .detach();
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let open = self.filter != Filter::All;
        self.query(
            move |f, token| forge::list(f, token, open),
            |this, list, cx| {
                this.list = list;
                // 选中的那项还在就跟着刷新详情
                match this.sel.as_ref().and_then(|s| this.list.iter().find(|r| r.number == s.number)).cloned() {
                    Some(r) => this.select(r, cx),
                    None => this.sel = None,
                }
            },
            cx,
        );
    }

    fn set_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
        // 只有「全部」要连已关闭的一起拉，其余在打开的里面筛
        let refetch = (filter == Filter::All) != (self.filter == Filter::All);
        self.filter = filter;
        if refetch {
            self.reload(cx);
        }
        cx.notify();
    }

    fn select(&mut self, r: Request, cx: &mut Context<Self>) {
        let Some(f) = self.forge.clone() else { return };
        self.request += 1;
        let (request, token, session) = (self.request, self.token.clone(), self.session.clone());
        self.sel = Some(r.clone());
        self.status = None;
        self.commits.clear();
        self.files_error.clear();
        self.files.update(cx, |d, cx| d.set(None, Source::Commit, String::new(), cx));
        cx.notify();
        crate::git::bg(
            cx,
            move || {
                let status = forge::status(&f, &token, &r);
                let notes = forge::notes(&f, &token, &r);
                let base = status.as_ref().map(|s| s.base.clone()).unwrap_or_default();
                // 提交和文件在本地算，缺的提交先从远程取
                let local = forge::fetch(&session.repo, &f, &r, &base).and_then(|(base, head)| Ok((forge::commits(&session.repo, &base, &head)?, engine::tools::compare(&session.repo, &base, &head, true)?)));
                (r, status, notes, local)
            },
            move |this, (r, status, notes, local), cx| {
                if this.request != request {
                    return;
                }
                match status {
                    Ok(s) => this.status = Some(s),
                    Err(e) => this.error = error_text(cx, &e),
                }
                let notes = notes.unwrap_or_else(|e| {
                    this.error = error_text(cx, &e);
                    Vec::new()
                });
                match local {
                    Ok((commits, c)) => {
                        this.commits = commits;
                        let detail = engine::detail::Detail { id: c.right.clone(), parents: vec![c.left.clone()], author: String::new(), author_email: String::new(), author_time: 0, committer: String::new(), committer_time: 0, message: r.title.clone(), files: c.files };
                        let path = this.repo.upgrade().map(|r| r.read(cx).path.clone()).unwrap_or_default();
                        let key = format!("review:{}", serde_json::to_string(&(&path, &c.left, &c.right)).unwrap());
                        this.files.update(cx, |d, cx| {
                            d.notes = notes;
                            d.set(Some(detail), Source::Compare(c.left, c.right), key, cx)
                        });
                    }
                    Err(e) => this.files_error = error_text(cx, &e),
                }
                cx.notify();
            },
        )
        .detach();
    }

    /// 平台上的状态变了：重新拉列表，再取一次远程让提交图跟上
    fn after_write(&mut self, cx: &mut Context<Self>) {
        self.reload(cx);
        let _ = self.repo.update(cx, |r, cx| r.exec(Op::Fetch { background: true }, false, cx).detach());
    }

    fn paste_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = self.forge.clone() else { return };
        let t = t(cx);
        let scope = if f.kind == Kind::GitHub { "repo" } else { "api" };
        let spec = Spec::new(t.requests.token_title).message((t.requests.token_hint)(&f.host, scope)).field(Field::text("token", t.requests.token_field).masked());
        self.ask(spec, window, cx, move |this, v, window, cx| {
            let token = v.text("token").trim().to_owned();
            this.busy = true;
            this.error.clear();
            cx.notify();
            cx.spawn_in(window, async move |this, cx| {
                let checked = cx
                    .background_spawn(async move {
                        let me = forge::user(&f, &token)?;
                        credential::save(&f.host, &token)?;
                        Ok::<_, String>((token, me))
                    })
                    .await;
                let _ = this.update_in(cx, |this, window, cx| {
                    this.busy = false;
                    match checked {
                        Ok((token, me)) => this.signed_in(token, me, window, cx),
                        Err(e) => this.error = error_text(cx, &e),
                    }
                    cx.notify();
                });
            })
            .detach();
        });
    }

    /// 打开令牌创建页，名字和权限已经填好
    fn token_page(&self, cx: &mut Context<Self>) {
        let Some(f) = &self.forge else { return };
        cx.open_url(&match f.kind {
            Kind::GitHub => format!("{}/settings/tokens/new?scopes=repo&description=PushRight", f.web),
            Kind::GitLab => format!("{}/-/profile/personal_access_tokens?name=PushRight&scopes=api", f.web),
        });
    }

    /// 同一仓库的 PR 切到（或跟踪）它的源分支；fork 来的从取回的头部提交建 pr-编号 分支
    fn checkout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(r), Some(f)) = (self.sel.clone(), self.forge.clone()) else { return };
        let name = format!("{}-{}", if f.kind == Kind::GitHub { "pr" } else { "mr" }, r.number);
        let _ = self.repo.update(cx, |repo, cx| {
            let has = |n: &str| repo.refs.refs.iter().any(|x| x.name == n);
            let (local, remote, own) = (format!("refs/heads/{}", r.source), format!("refs/remotes/{}/{}", f.remote, r.source), format!("refs/heads/{name}"));
            if !r.fork && has(&local) {
                repo.activate_ref(RefKind::Branch, local, window, cx)
            } else if !r.fork && has(&remote) {
                repo.activate_ref(RefKind::Remote, remote, window, cx)
            } else if has(&own) {
                repo.activate_ref(RefKind::Branch, own, window, cx)
            } else {
                repo.exec(Op::CreateBranch { name, start: r.head.clone(), checkout: true }, true, cx).detach()
            }
        });
    }

    fn merge(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(r), Some(f), Some(s)) = (self.sel.clone(), self.forge.clone(), self.status.clone()) else { return };
        let t = t(cx);
        // GitHub 只列仓库允许的合并方式；GitLab 的压缩和删源分支按服务器上的设置预先勾好
        let fields = match f.kind {
            Kind::GitHub => vec![Field::radio("method", t.requests.method, s.methods.iter().map(|m| (m.clone(), lookup(t.requests.methods, m).unwrap_or(m).to_owned())).collect()).value(s.methods.first().cloned().unwrap_or_default())],
            Kind::GitLab => vec![Field::checkbox("squash", t.requests.squash, s.squash), Field::checkbox("remove", t.requests.remove_source, s.remove_source)],
        };
        let spec = Spec::new((t.requests.merge_title)(r.number)).message(format!("{} → {}", r.source, r.target)).maybe_warning(self.outsider(cx)).confirm(t.requests.merge).fields(fields);
        self.ask(spec, window, cx, move |this, v, _, cx| {
            let (method, squash, remove) = (v.text("method"), v.bool("squash"), v.bool("remove"));
            this.query(move |f, token| forge::merge(f, token, &r, &method, squash, remove), |this, _, cx| this.after_write(cx), cx);
        });
    }

    /// 不用确认，直接批准；要提醒时先问一下
    fn approve(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(n) = self.sel.as_ref().map(|r| r.number) else { return };
        let run = move |this: &mut Self, cx: &mut Context<Self>| this.query(move |f, token| forge::approve(f, token, n), |this, _, cx| this.after_write(cx), cx);
        match self.outsider(cx) {
            Some(w) => {
                let t = t(cx);
                self.ask(Spec::new((t.requests.approve_title)(n)).warning(w).confirm(t.requests.approve), window, cx, move |this, _, _, cx| run(this, cx));
            }
            None => run(self, cx),
        }
    }

    /// 取项目成员后弹单选，默认是现在的审核者；服务器没改（没有权限）时报错
    fn change_reviewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(r), Some(f)) = (self.sel.clone(), self.forge.clone()) else { return };
        let token = self.token.clone();
        self.busy = true;
        self.error.clear();
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let found = cx.background_spawn(async move { forge::members(&f, &token).map(|m| (f, m)) }).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                let (f, members) = match found {
                    Ok(v) => v,
                    Err(e) => {
                        this.error = error_text(cx, &e);
                        return cx.notify();
                    }
                };
                let t = t(cx);
                let now = r.reviewers.first().map_or("0".into(), |n| id_of(&members, n));
                let field = Field::select("reviewer", t.requests.reviewer, people(&members, &recent(&f, cx), cx)).value(now);
                this.ask(Spec::new((t.requests.reviewer_title)(r.number)).field(field), window, cx, move |this, v, _, cx| {
                    let (n, id) = (r.number, v.text("reviewer").parse().unwrap_or(0));
                    remember(&f, &members, id, cx);
                    this.query(move |f, token| forge::set_reviewer(f, token, n, id), |this, _, cx| this.after_write(cx), cx);
                });
            });
        })
        .detach();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(n) = self.sel.as_ref().map(|r| r.number) else { return };
        let t = t(cx);
        let warning = self.outsider(cx).map_or(t.requests.close_warning.to_owned(), |w| format!("{w}\n{}", t.requests.close_warning));
        let spec = Spec::new((t.requests.close_title)(n)).warning(warning).confirm(t.requests.close).danger();
        self.ask(spec, window, cx, move |this, _, _, cx| this.query(move |f, token| forge::close(f, token, n), |this, _, cx| this.after_write(cx), cx));
    }

    /// 对选中的行发评论；发完只重取评论，diff 停在原处。没发出去就带着原文和原因重新弹出：禅模式里看不到页面上的错误
    fn comment(&mut self, a: Anchor, body: String, failed: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(r), Some(st)) = (self.sel.clone(), self.status.clone()) else { return };
        let t = t(cx);
        // 标题里的行号：删除行是旧文件的，其余是新文件的
        let no = |p: &Spot| if p.kind == '-' { p.old } else { p.new };
        let warning = [failed, self.outsider(cx)].into_iter().flatten().collect::<Vec<_>>().join("\n");
        let spec = Spec::new((t.requests.comment_title)(no(&a.from), no(&a.to))).message(a.path.clone()).maybe_warning(Some(warning).filter(|w| !w.is_empty())).field(Field::textarea("body", t.requests.comment_body).value(body)).confirm(t.requests.comment);
        self.ask(spec, window, cx, move |this, v, window, cx| {
            let Some(f) = this.forge.clone() else { return };
            let (token, body) = (this.token.clone(), v.text("body"));
            this.busy = true;
            this.error.clear();
            cx.notify();
            cx.spawn_in(window, async move |this, cx| {
                let (a2, b) = (a.clone(), body.clone());
                // 发出去了才重取评论；重取失败不算没发出去，免得重发
                let sent = cx.background_spawn(async move { forge::comment(&f, &token, &r, &st, &a2, &b).map(|_| forge::notes(&f, &token, &r)) }).await;
                let _ = this.update_in(cx, |this, window, cx| {
                    this.busy = false;
                    match sent {
                        Ok(Ok(notes)) => this.files.update(cx, |d, cx| d.set_notes(notes, cx)),
                        Ok(Err(e)) => this.error = error_text(cx, &e),
                        Err(e) => this.comment(a, body, Some(error_text(cx, &e)), window, cx),
                    }
                    cx.notify();
                });
            })
            .detach();
        });
    }

    /// GitLab 上指定了审核者、自己既不是审核者也不是作者时的提醒
    fn outsider(&self, cx: &App) -> Option<String> {
        let r = self.sel.as_ref()?;
        let gitlab = self.forge.as_ref().is_some_and(|f| f.kind == Kind::GitLab);
        (gitlab && !r.reviewers.is_empty() && !r.reviewers.contains(&self.me) && r.author != self.me).then(|| (t(cx).requests.not_reviewer)(r.number, &r.reviewers.join("、")))
    }

    /// 为分支新建：目标默认是仓库的默认分支；一个提交时标题用它的说明，多个时标题用分支名、说明里列出各提交。没推送的先推送
    pub fn create(&mut self, branch: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = self.forge.clone().filter(|_| !self.token.is_empty()) else {
            self.pending = Some(branch);
            return;
        };
        let Some(repo) = self.repo.upgrade() else { return };
        let refs = &repo.read(cx).refs.refs;
        let find = |name: String| refs.iter().find(|x| x.name == name);
        let local = find(format!("refs/heads/{branch}"));
        let pushed = local.is_some() && local.map(|x| &x.id) == find(format!("refs/remotes/{}/{branch}", f.remote)).map(|x| &x.id);
        let tracked = local.is_some_and(|x| x.upstream.is_some());
        let prefix = format!("refs/remotes/{}/", f.remote);
        let mut targets: Vec<String> = refs.iter().filter_map(|x| x.name.strip_prefix(&prefix)).filter(|n| *n != "HEAD" && *n != branch).map(str::to_owned).collect();
        let (session, token) = (self.session.clone(), self.token.clone());
        self.busy = true;
        self.error.clear();
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let b = branch.clone();
            let found = cx
                .background_spawn(async move {
                    let p = forge::project(&f, &token)?;
                    let commits = forge::commits(&session.repo, &format!("refs/remotes/{}/{}", f.remote, p.default_branch), &format!("refs/heads/{b}")).unwrap_or_default();
                    let members = if f.kind == Kind::GitLab { forge::members(&f, &token)? } else { Vec::new() };
                    Ok::<_, String>((f, p, commits, members))
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                let (f, p, commits, members) = match found {
                    Ok(v) => v,
                    Err(e) => {
                        this.error = error_text(cx, &e);
                        return cx.notify();
                    }
                };
                let t = t(cx);
                let (title, body) = match commits.as_slice() {
                    [one] => (one.subject.clone(), String::new()),
                    many => (branch.clone(), many.iter().rev().map(|c| format!("- {}", c.subject)).collect::<Vec<_>>().join("\n")),
                };
                if !targets.contains(&p.default_branch) {
                    targets.insert(0, p.default_branch.clone());
                }
                let mut fields = vec![
                    Field::select("target", t.requests.target, targets.iter().map(|b| (b.clone(), b.clone())).collect()).value(p.default_branch.clone()),
                    Field::text("title", t.requests.title).value(title),
                    Field::textarea("body", t.requests.body).optional().value(body),
                    Field::checkbox("draft", t.requests.draft, false),
                ];
                if f.kind == Kind::GitLab {
                    let recent = recent(&f, cx);
                    let reviewer = recent.iter().map(|n| id_of(&members, n)).find(|id| id != "0").unwrap_or_else(|| "0".into());
                    let options = people(&members, &recent, cx);
                    fields.push(Field::select("assignee", t.requests.assignee, options.clone()).value(id_of(&members, &this.me)));
                    fields.push(Field::select("reviewer", t.requests.reviewer, options).value(reviewer));
                    fields.push(Field::checkbox("remove", t.requests.remove_source, p.remove_source));
                    fields.push(Field::checkbox("squash", t.requests.squash, p.squash));
                }
                if !pushed {
                    fields.push(Field::checkbox("push", (t.requests.push_first)(&format!("{}/{branch}", f.remote)), true));
                }
                let noun = if f.kind == Kind::GitHub { t.requests.pr } else { t.requests.mr };
                let spec = Spec::new((t.requests.create_title)(noun, &branch)).confirm(t.requests.create).fields(fields);
                this.ask(spec, window, cx, move |this, v, _, cx| {
                    let n = NewRequest { source: branch.clone(), target: v.text("target"), title: v.text("title"), body: v.text("body"), draft: v.bool("draft"), remove_source: v.bool("remove"), squash: v.bool("squash"), assignee: v.text("assignee").parse().unwrap_or(0), reviewer: v.text("reviewer").parse().unwrap_or(0) };
                    remember(&f, &members, n.reviewer, cx);
                    let push = v.bool("push").then(|| Op::Push { remote: f.remote.clone(), branch: branch.clone(), remote_branch: branch.clone(), force: false, set_upstream: !tracked, tags: false });
                    let pushing = push.and_then(|op| this.repo.update(cx, |r, cx| r.exec(op, true, cx)).ok());
                    cx.spawn(async move |this, cx| {
                        if let Some(pushing) = pushing {
                            if !pushing.await {
                                return;
                            }
                        }
                        let _ = this.update(cx, |this, cx| {
                            this.query(
                                move |f, token| forge::create(f, token, &n),
                                |this, r, cx| {
                                    this.filter = Filter::Open;
                                    this.sel = Some(r);
                                    this.reload(cx);
                                },
                                cx,
                            )
                        });
                    })
                    .detach();
                });
            });
        })
        .detach();
    }

    fn render_row(&mut self, r: &Request, cx: &mut Context<Self>) -> AnyElement {
        let p = palette(cx).clone();
        let on = self.sel.as_ref().is_some_and(|s| s.number == r.number);
        let updated = r.updated.parse::<jiff::Timestamp>().map(|t| fmt_time(cx, t.as_second())).unwrap_or_default();
        let r2 = r.clone();
        div()
            .id(("request", r.number as usize))
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(2.))
            .w_full()
            .h(px(ROW_H))
            .px(px(12.))
            .border_b_1()
            .border_color(p.border)
            .when(on, |s| s.bg(p.accent_soft))
            .when(!on, |s| s.hover(|s| s.bg(p.hover)))
            .on_click(cx.listener(move |this, _, _, cx| this.select(r2.clone(), cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(dot(state_color(state(r), &p)))
                    .child(div().flex_1().min_w_0().truncate().font_weight(FontWeight::SEMIBOLD).child(r.title.clone()))
                    .child(div().flex_none().font_family(MONO).text_size(FS_SM).text_color(p.muted).child(format!("#{}", r.number))),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .pl(px(16.))
                    .text_size(FS_SM)
                    .text_color(p.muted)
                    .child(div().min_w_0().truncate().font_family(MONO).child(format!("{} → {}", r.source, r.target)))
                    .child(div().flex_1())
                    .child(div().flex_none().child(format!("{} · {updated}", r.author))),
            )
            .into_any_element()
    }

    fn render_list(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let busy = self.busy;
        let mut bar = div().flex().flex_wrap().items_center().gap(px(4.)).px(px(8.)).py(px(8.)).border_b_1().border_color(p.border);
        for (i, (filter, text)) in [(Filter::Open, t.requests.open), (Filter::Mine, t.requests.mine), (Filter::Review, t.requests.review), (Filter::All, t.requests.all)].into_iter().enumerate() {
            bar = bar.child(btn(("request-filter", i)).small().text(text).on(self.filter == filter).on_click(cx.listener(move |this, _, _, cx| this.set_filter(filter, cx))));
        }
        bar = bar
            .child(div().flex_1())
            .child(btn("request-refresh").small().quiet().icon(Icon::Fetch).tip(t.requests.refresh).disabled(busy).on_click(cx.listener(|this, _, _, cx| this.reload(cx))))
            .child(btn("request-token").small().quiet().icon(Icon::Lock).tip(t.requests.token_change).on_click(cx.listener(|this, _, window, cx| this.paste_token(window, cx))))
            .child(btn("request-new").small().icon(Icon::Plus).text(t.requests.create).disabled(busy).on_click(cx.listener(|this, _, window, cx| {
                let head = this.repo.upgrade().and_then(|r| r.read(cx).head());
                match head {
                    Some(b) => this.create(b, window, cx),
                    None => {
                        this.error = t.no_branch.into();
                        cx.notify();
                    }
                }
            })));
        let me = self.me.clone();
        let filter = self.filter;
        let shown: Rc<Vec<usize>> = Rc::new((0..self.list.len()).filter(|&i| match filter {
            Filter::Mine => self.list[i].author == me,
            Filter::Review => self.list[i].reviewers.contains(&me),
            _ => true,
        }).collect());
        let body = if shown.is_empty() {
            div().flex_1().p(px(24.)).text_color(p.muted).font_family(PROSE).child(if busy { t.requests.connecting } else { t.requests.empty }).into_any_element()
        } else {
            let list = uniform_list(
                "requests",
                shown.len(),
                cx.processor(move |this: &mut Requests, range: Range<usize>, _window: &mut Window, cx: &mut Context<Requests>| {
                    range.map(|ix| {
                        let r = this.list[shown[ix]].clone();
                        this.render_row(&r, cx)
                    }).collect::<Vec<_>>()
                }),
            )
            .track_scroll(&self.scroll)
            .size_full();
            div().relative().flex_1().min_h_0().child(list).vertical_scrollbar(&self.scroll).into_any_element()
        };
        div().flex().flex_col().flex_none().w(px(layout(cx).requests)).min_h_0().child(bar).child(body).into_any_element()
    }

    fn render_detail(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let Some(r) = self.sel.clone() else {
            return div().flex_1().flex().items_center().justify_center().text_color(p.muted).font_family(PROSE).text_size(FS_LG).child(t.requests.pick).into_any_element();
        };
        let busy = self.busy;
        let open = r.state == "open";
        let st = state(&r).to_owned();
        let color = state_color(&st, &p);
        let chip = div().px(px(8.)).rounded(R_LG).bg(alpha(color, 0.18)).text_color(color).child(lookup(t.requests.states, &st).unwrap_or(&st).to_owned());
        let (url, link) = (r.url.clone(), r.url.clone());
        let actions = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(btn("request-checkout").small().text(t.requests.checkout).disabled(busy).on_click(cx.listener(|this, _, window, cx| this.checkout(window, cx))))
            .child(btn("request-merge").small().primary().text(t.requests.merge).disabled(busy || !open || self.status.is_none()).on_click(cx.listener(|this, _, window, cx| this.merge(window, cx))))
            .child(btn("request-approve").small().text(t.requests.approve).disabled(busy || !open).on_click(cx.listener(|this, _, window, cx| this.approve(window, cx))))
            .when(self.forge.as_ref().is_some_and(|f| f.kind == Kind::GitLab), |d| d.child(btn("request-reviewer").small().text(t.requests.change_reviewer).disabled(busy || !open).on_click(cx.listener(|this, _, window, cx| this.change_reviewer(window, cx)))))
            .child(btn("request-close").small().danger().text(t.requests.close).disabled(busy || !open).on_click(cx.listener(|this, _, window, cx| this.close(window, cx))))
            .child(div().flex_1())
            .child(btn("request-browser").small().quiet().text(t.requests.browser).on_click(move |_, _, cx| cx.open_url(&url)))
            .child(btn("request-copy").small().quiet().text(t.requests.copy_link).on_click(move |_, _, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(link.clone()))));
        let files = self.files.read(cx).file_count();
        let tabs = div()
            .flex()
            .gap(px(4.))
            .child(btn("request-overview").small().quiet().text(t.requests.overview).on(!self.show_files).on_click(cx.listener(|this, _, _, cx| {
                this.show_files = false;
                cx.notify();
            })))
            .child(btn("request-files").small().quiet().text((t.requests.files)(files)).on(self.show_files).on_click(cx.listener(|this, _, _, cx| {
                this.show_files = true;
                cx.notify();
            })));
        let header = div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(10.))
            .px(px(24.))
            .pt(px(16.))
            .pb(px(8.))
            .border_b_1()
            .border_color(p.border)
            .child(div().flex().items_baseline().gap(px(8.)).child(div().min_w_0().font_family(DISPLAY).text_size(FS_LG).font_weight(FontWeight::SEMIBOLD).child(r.title.clone())).child(div().flex_none().font_family(MONO).text_color(p.muted).child(format!("#{}", r.number))))
            .child(div().flex().flex_wrap().items_center().gap(px(8.)).text_size(FS_SM).child(chip).child(div().text_color(p.muted).child(r.author.clone())).child(div().font_family(MONO).child(format!("{} → {}", r.source, r.target))))
            .child(actions)
            .child(tabs);
        let body = if self.show_files {
            if self.files_error.is_empty() {
                div().flex().flex_1().min_h_0().child(self.files.clone()).into_any_element()
            } else {
                div().flex_1().p(px(24.)).text_color(p.red).child(self.files_error.clone()).into_any_element()
            }
        } else {
            self.render_overview(&r, cx)
        };
        div().flex().flex_col().flex_1().min_w_0().min_h_0().child(header).child(body).into_any_element()
    }

    fn render_overview(&mut self, r: &Request, cx: &mut Context<Self>) -> AnyElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let heading = |text: String| div().mt(px(20.)).mb(px(8.)).text_size(FS_SM).font_weight(FontWeight::SEMIBOLD).text_color(p.muted).child(text);
        let description = if r.body.trim().is_empty() {
            div().text_color(p.muted).child(t.requests.no_description)
        } else {
            div().max_w(px(72. * 9.)).font_family(PROSE).line_height(gpui::relative(LH_BODY)).child(r.body.clone())
        };
        let mut page = div().id("request-overview").flex_1().min_h_0().px(px(24.)).pb(px(24.)).child(description);
        match &self.status {
            None => page = page.child(heading(t.requests.checks.into())).child(div().text_color(p.muted).child(t.requests.connecting)),
            Some(s) => {
                page = page.child(heading(t.requests.checks.into()));
                if s.checks.is_empty() {
                    page = page.child(div().text_color(p.muted).child(t.requests.no_checks));
                }
                for (i, c) in s.checks.iter().enumerate() {
                    let url = c.url.clone();
                    page = page.child(
                        div()
                            .id(("check", i))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .h(px(24.))
                            .when(!url.is_empty(), |d| d.cursor_pointer().hover(|s| s.text_color(p.accent)).on_click(move |_, _, cx| cx.open_url(&url)))
                            .child(dot(state_color(&c.state, &p)))
                            .child(div().min_w_0().truncate().child(c.name.clone())),
                    );
                }
                page = page.child(heading(t.requests.reviews.into()));
                if !r.reviewers.is_empty() {
                    page = page.child(div().child((t.requests.reviewer_is)(&r.reviewers.join("、"))));
                }
                if s.approved.is_empty() && s.changes_requested.is_empty() {
                    page = page.child(div().text_color(p.muted).child(t.requests.no_reviews));
                }
                if !s.approved.is_empty() {
                    page = page.child(div().text_color(p.green).child((t.requests.approved)(&s.approved.join("、"))));
                }
                if !s.changes_requested.is_empty() {
                    page = page.child(div().text_color(p.red).child((t.requests.changes)(&s.changes_requested.join("、"))));
                }
                if r.state == "open" && !s.mergeable.is_empty() {
                    page = page.child(heading(t.requests.merge_state.into())).child(div().child(lookup(t.requests.merge_states, &s.mergeable).unwrap_or(&s.mergeable).to_owned()));
                }
            }
        }
        page = page.child(heading((t.requests.commits)(self.commits.len())));
        for c in &self.commits {
            page = page.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .h(px(24.))
                    .text_size(FS_SM)
                    .child(div().flex_none().font_family(MONO).text_color(p.muted).child(c.id[..8.min(c.id.len())].to_owned()))
                    .child(div().flex_1().min_w_0().truncate().child(c.subject.clone()))
                    .child(div().flex_none().text_color(p.muted).child(c.author.clone())),
            );
        }
        page.overflow_y_scrollbar().into_any_element()
    }
}

impl Render for Requests {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx).clone();
        let t = t(cx);
        let page = div().flex().flex_1().min_w_0().min_h_0().bg(p.panel).text_size(FS_MD);
        let error = (!self.error.is_empty()).then(|| div().px(px(24.)).py(px(8.)).text_color(p.red).child(self.error.clone()));
        // 还没连上：连接中、认不出平台，或者要用户给令牌
        let notice = |title: &str, hint: String| div().flex().flex_col().gap(px(12.)).m_auto().max_w(px(560.)).child(div().font_family(DISPLAY).text_size(FS_LG).font_weight(FontWeight::SEMIBOLD).child(title.to_owned())).child(div().font_family(PROSE).text_color(p.muted).line_height(gpui::relative(LH_BODY)).child(hint));
        let Some(f) = self.forge.clone() else {
            let body = if self.connecting {
                notice(t.requests.connecting, String::new())
            } else {
                notice(t.requests.nav, t.requests.forge_hint.into()).child(div().child(btn("request-retry").text(t.requests.retry).on_click(cx.listener(|this, _, window, cx| this.connect(window, cx)))))
            };
            return page.flex_col().children(error).child(body).into_any_element();
        };
        if self.token.is_empty() {
            let scope = if f.kind == Kind::GitHub { "repo" } else { "api" };
            let body = notice(t.requests.token_title, (t.requests.token_hint)(&f.host, scope)).child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(btn("token-new").text(t.requests.token_new).on_click(cx.listener(|this, _, _, cx| this.token_page(cx))))
                    .child(btn("token-paste").primary().text(t.requests.token_paste).disabled(self.busy).on_click(cx.listener(|this, _, window, cx| this.paste_token(window, cx)))),
            );
            return page.flex_col().children(error).child(body).into_any_element();
        }
        let list = self.render_list(cx);
        let detail = self.render_detail(cx);
        page.child(list).child(layout::splitter("split-requests", Key::Requests, true, 280., 720., false, cx)).child(div().flex().flex_col().flex_1().min_w_0().min_h_0().children(error).child(detail)).into_any_element()
    }
}
