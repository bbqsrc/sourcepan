use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::cmp::min;
use std::sync::mpsc::{channel, TryRecvError};
use std::time::Duration;

use notify::{DebouncedEvent, RecommendedWatcher, Watcher, RecursiveMode};
use chrono::{self, TimeZone};
use git2;
use gtk::prelude::*;
use gtk;

use super::branch::{BranchPresenter, BranchView};
use super::CommitInfo;

pub trait HistoryViewable {
    fn new(parent: Weak<BranchPresenter<BranchView>>) -> Rc<Self>;
    fn set_history(&self, commits: &[CommitInfo]);
    fn selected_row(&self) -> Option<usize>;
}

struct HistoryPresenter<V> {
    parent: Weak<BranchPresenter<BranchView>>,
    view: RefCell<Weak<V>>,
    commits: RefCell<Vec<CommitInfo>>,
    watcher: RefCell<RecommendedWatcher>
}

impl<V: HistoryViewable> HistoryPresenter<V> where V: 'static {
    fn new(parent: Weak<BranchPresenter<BranchView>>) -> Rc<HistoryPresenter<V>> {
        let (tx, rx) = channel();

        let presenter = Rc::new(HistoryPresenter {
            parent: parent,
            view: RefCell::new(Weak::new()),
            commits: RefCell::new(vec![]),
            watcher: RefCell::new(Watcher::new(tx, Duration::from_secs(2)).unwrap())
        });

        gtk::idle_add(weak!(presenter => move || {
            match rx.try_recv() {
                Err(err) => {
                    match err {
                        TryRecvError::Empty => gtk::Continue(true),
                        TryRecvError::Disconnected => {
                            gtk::Continue(false)
                        }
                    }
                },
                Ok(v) => {
                    match presenter.upgrade() {
                        Some(p) => {
                            p.on_path_change_event(v);
                            gtk::Continue(true)
                        },
                        None => {
                            gtk::Continue(false)
                        }
                    }
                }
            }
        }));

        presenter
    }

    fn on_path_change_event(&self, event: DebouncedEvent) {
        let maybe_path = match event {
            DebouncedEvent::NoticeWrite(path) => Some(path),
            DebouncedEvent::NoticeRemove(path) => Some(path),
            DebouncedEvent::Create(path) => Some(path),
            DebouncedEvent::Write(path) => Some(path),
            DebouncedEvent::Chmod(path) => Some(path),
            DebouncedEvent::Remove(path) => Some(path),
            DebouncedEvent::Rename(_, new_path) => Some(new_path),
            DebouncedEvent::Rescan => None,
            DebouncedEvent::Error(_, maybe_path) => maybe_path
        };

        let path = match maybe_path {
            Some(v) => v,
            None => return
        };

        let parent = self.parent();
        if path.ends_with("index") || path.ends_with(&parent.branch()) || !path.components().any(|x| x.as_os_str() == ".git") {
            self.load_commit_history();
            if let Some(idx) = self.view().selected_row() {
                self.on_item_selected(idx);
            }
        }
    }

    fn view(&self) -> Rc<V> {
        self.view.borrow()
            .upgrade()
            .expect("Presenter only running while view still exists")
    }

    fn parent(&self) -> Rc<BranchPresenter<BranchView>> {
        self.parent
            .upgrade()
            .expect("Presenter only running while parent still exists")
    }

    fn has_uncommitted_changes(&self) -> bool {
        let parent = self.parent();
        let statuses = parent.repo().statuses(None).unwrap();
        statuses.iter().filter(|x| !x.status().is_ignored()).count() > 0
    }

    pub fn load_commit_history(&self) {
        let parent = self.parent();
        let branch = &parent.repo().find_branch(&self.parent().branch(), git2::BranchType::Local).expect("find branch");
        let refr = branch.get().name().expect("find branch name as ref");
        let mut revwalk = (&parent.repo()).revwalk().expect("get a revwalk");
        revwalk.push_ref(refr).expect("push ref successfully");

        let mut infos = vec![];

        if self.has_uncommitted_changes() {
            infos.push(CommitInfo::uncommitted_sentinel());
        }

        for rev in revwalk {
            let rev = match rev {
                Ok(v) => v,
                Err(_) => continue
            };

            let commit = (&parent.repo()).find_commit(rev).expect("commit to be found");

            let author = commit.author();
            let subid = &format!("{}", commit.id())[0..7];
            let full_summary = commit.summary().expect("valid summary");
            let summary = &format!("{}", &full_summary)[0..min(80, full_summary.len())];

            let naive_dt = chrono::Utc.timestamp(commit.time().seconds(), 0).naive_utc();
            let offset = chrono::offset::FixedOffset::east(commit.time().offset_minutes() * 60);
            let date: chrono::DateTime<chrono::FixedOffset> = chrono::DateTime::from_utc(naive_dt, offset);

            let author_name = author.name().expect("valid author name");
            let author_email = author.email().expect("valid author email");

            let info = CommitInfo {
                id: commit.id(),
                summary: summary.to_string(),
                short_id: subid.to_string(),
                author: format!("{} <{}>", &author_name, &author_email),
                commit_date: date.to_string()
            };

            infos.push(info);
        }

        *self.commits.borrow_mut() = infos;

        self.view().set_history(&self.commits.borrow());
    }

    fn on_item_selected(&self, index: usize) {
        let info = &self.commits.borrow()[index];

        if info.is_sentinel() {
            self.parent().on_uncommitted_changes_selected();
        } else {
            self.parent().on_commit_selected(&info)
        }
    }

    fn start(&self) {
        self.load_commit_history();
        let parent = self.parent();

        self.watcher.borrow_mut()
            .watch(parent.repo().path().parent().unwrap(), RecursiveMode::Recursive)
            .unwrap();
    }
}

pub struct HistoryView {
    presenter: Rc<HistoryPresenter<HistoryView>>,
    list_store: gtk::ListStore,
    tree: gtk::TreeView,
    root: gtk::ScrolledWindow
}

impl HistoryView {
    fn create_tree(model: &gtk::ListStore) -> gtk::TreeView {
        fn append_column(tree: &gtk::TreeView, id: i32, title: &str) {
            let column = gtk::TreeViewColumn::new();
            let cell = gtk::CellRendererText::new();

            column.pack_start(&cell, true);
            column.set_resizable(true);
            column.set_title(title);
            column.add_attribute(&cell, "text", id);
            tree.append_column(&column);
        }

        let treeview = gtk::TreeView::new();

        append_column(&treeview, 0, "Summary");
        append_column(&treeview, 1, "Commit");
        append_column(&treeview, 2, "Author");
        append_column(&treeview, 3, "Date");

        treeview.set_model(model);
        treeview
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }
}

impl HistoryViewable for HistoryView {
    fn new(parent: Weak<BranchPresenter<BranchView>>) -> Rc<HistoryView> {
        let list_store = gtk::ListStore::new(&[
            String::static_type(),
            String::static_type(),
            String::static_type(),
            String::static_type()
        ]);

        let treeview = HistoryView::create_tree(&list_store);

        // Make tree view scrollable
        let root = gtk::ScrolledWindow::new(None, None);
        root.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        root.add(&treeview);

        let view = view!(HistoryView {
            presenter: HistoryPresenter::new(parent),
            list_store: list_store,
            tree: treeview,
            root: root
        });

        view.tree.connect_cursor_changed(weak!(view => move |_| {
            if let Some(view) = view.upgrade() {
                if let Some(idx) = view.selected_row() {
                    view.presenter.on_item_selected(idx);
                }
            } else {
                panic!("HistoryView not found in weak reference counter for tree selection");
            }
        }));

        view.presenter.start();
        
        view
    }

    fn selected_row(&self) -> Option<usize> {
        if let Some(path) = self.tree.get_cursor().0 {
            if let Some(idx) = path.get_indices().first() {
                return Some(*idx as usize);
            }
        }

        None
    }

    fn set_history(&self, commits: &[CommitInfo]) {
        let cursor = self.tree.get_cursor();

        self.list_store.clear();

        for commit in commits {
            self.list_store.insert_with_values(None, &[0, 1, 2, 3], &[
                &commit.summary,
                &commit.short_id,
                &commit.author,
                &commit.commit_date
            ]);
        }

        let col = match cursor.1 {
            Some(ref v) => Some(v),
            None => None
        };

        if let Some(path) = cursor.0 {
            self.tree.set_cursor(&path, col, false);
        }
    }
}
