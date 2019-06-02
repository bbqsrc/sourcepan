// Sourcepan - a Gtk+ Git client written in Rust
// Copyright (C) 2018  Brendan Molloy <brendan@bbqsrc.net>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 3 as 
// published by the Free Software Foundation.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::cmp::min;
use std::sync::mpsc::{channel, TryRecvError};
use std::time::Duration;
use std::fmt;

use notify::{DebouncedEvent, RecommendedWatcher, Watcher, RecursiveMode};
use chrono::{self, TimeZone};
use git2;
use gtk::prelude::*;
use gtk;
use glib::markup_escape_text;
use pango;

use super::branch::{BranchPresenter, BranchView, BranchViewable};
use super::CommitInfo;

pub trait HistoryViewable {
    fn new(parent: Weak<BranchPresenter<BranchView>>) -> Rc<Self>;
    fn set_history(&self, commits: &[CommitInfo]);
    fn selected_row(&self) -> Option<usize>;
    fn handle_error(&self, error: impl fmt::Display);
    fn refresh_commit_history(&self);
}

struct HistoryPresenter<V> {
    parent: Weak<BranchPresenter<BranchView>>,
    view: RefCell<Weak<V>>,
    commits: RefCell<Vec<CommitInfo>>,
    watcher: RefCell<RecommendedWatcher>
}

pub trait HumanCommitExt<'a> {
    fn author_str(&self) -> String;
    fn id_str(&self) -> String;
    fn short_id_str(&self) -> String;
    fn full_summary_str(&'a self) -> &'a str;
    fn summary_str(&'a self) -> &'a str;
    fn date(&self) -> chrono::DateTime<chrono::FixedOffset>;
}

impl<'a> HumanCommitExt<'a> for git2::Commit<'a> {
    fn author_str(&self) -> String {
        let author = self.author();
        let author_name = author.name().unwrap_or("Unknown");
        let author_email = author.email().unwrap_or("unknown");
        format!("{} <{}>", &author_name, &author_email)
    }

    fn id_str(&self) -> String {
        format!("{}", self.id())
    }

    fn short_id_str(&self) -> String {
        self.id_str()[0..7].to_string()
    }
    
    fn full_summary_str(&'a self) -> &'a str {
        self.summary().unwrap_or("<No summary found>")
    }

    fn summary_str(&'a self) -> &'a str {
        let full_summary = self.full_summary_str();
        &full_summary[0..min(80, full_summary.len())]
    }

    fn date(&self) -> chrono::DateTime<chrono::FixedOffset> {
        let time = self.time();
        let naive_dt = chrono::Utc.timestamp(time.seconds(), 0).naive_utc();
        let offset = chrono::offset::FixedOffset::east(time.offset_minutes() * 60);
        let date: chrono::DateTime<chrono::FixedOffset> = chrono::DateTime::from_utc(naive_dt, offset);
        date
    }
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

        gtk::timeout_add(50, weak!(presenter => move || {
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
        if path.ends_with("index") || path.ends_with(&*parent.branch().borrow()) || !path.components().any(|x| x.as_os_str() == ".git") {
            self.update_commit_history();
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
        let repo = parent.repo();
        let statuses = repo.statuses(None).unwrap();
        statuses.iter().filter(|x| !x.status().is_ignored()).count() > 0
    }

    pub fn update_commit_history(&self) {
        let parent = self.parent();
        let repo = parent.repo();
        let branch = repo.find_branch(&parent.branch().borrow(), git2::BranchType::Local).expect("find branch");
        let refr = branch.get().name().expect("find branch name as ref");

        let revwalk = {
            let mut revwalk = repo.revwalk().expect("get a revwalk");
            revwalk.push_head().expect("head can be pushed");
            let first_commit = revwalk.next().unwrap().unwrap();

            let mut revwalk = repo.revwalk().expect("get a revwalk");
            let mut sort = git2::Sort::TIME;
            sort.insert(git2::Sort::TOPOLOGICAL);
            revwalk.set_sorting(sort);
            revwalk.push_ref(refr).expect("push ref successfully");
            revwalk.push(first_commit).unwrap();

            // These two may fail if the directories for them do not exist, so we unwrap and ignore.
            revwalk.push_glob("heads/*").unwrap_or(());
            revwalk.push_glob("remotes/*").unwrap_or(());
            
            revwalk
        };


        let mut infos = vec![];

        if self.has_uncommitted_changes() {
            infos.push(CommitInfo::uncommitted_sentinel());
        }

        let repo = parent.repo();
        let branches: Vec<(String, git2::Commit)> = repo.branches(None).unwrap()
            .map(|branch| {
                let branch = branch.unwrap().0;
                let name = branch.name().unwrap().unwrap().to_string();
                let commit = branch.get().peel_to_commit().unwrap();
                (name, commit)
            })
            .collect();

        for rev in revwalk {
            let rev = match rev {
                Ok(v) => v,
                Err(_) => continue
            };

            let commit = repo.find_commit(rev).expect("commit to be found");
            
            let branch_heads: Vec<String> = branches.iter()
                .filter(|(_, tip_commit)| tip_commit.id() == commit.id())
                .map(|(name, _)| name.clone())
                .collect();

            let info = CommitInfo {
                id: commit.id(),
                summary: commit.summary_str().to_string(),
                short_id: commit.short_id_str().to_string(),
                author: commit.author_str().to_string(),
                commit_date: commit.date().to_string(),
                branch_heads: branch_heads
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
        self.update_commit_history();
        let parent = self.parent();

        let result = self.watcher.borrow_mut()
            .watch(parent.repo().path().parent().unwrap(), RecursiveMode::Recursive);

        match result {
            Ok(_) => {},
            Err(err) => self.view().handle_error(err)
        }
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
        fn append_column(tree: &gtk::TreeView, id: i32, title: &str, is_expand: bool) {
            let column = gtk::TreeViewColumn::new();
            let cell = gtk::CellRendererText::new();

            column.pack_start(&cell, true);
            column.set_expand(is_expand);
            column.set_resizable(true);
            column.set_title(title);
            column.add_attribute(&cell, "markup", id);

            cell.set_property("width-chars", &12).unwrap();
            cell.set_property("ellipsize-set", &true).unwrap();
            cell.set_property("ellipsize", &pango::EllipsizeMode::End).unwrap();

            tree.append_column(&column);
        }

        let treeview = gtk::TreeView::new();

        append_column(&treeview, 0, "Summary", true);
        append_column(&treeview, 1, "Commit", false);
        append_column(&treeview, 2, "Author", true);
        append_column(&treeview, 3, "Date", true);

        treeview.set_model(model);
        treeview
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }

    fn add_to_tree_escaped(&self, commit: &CommitInfo) {
        self.list_store.insert_with_values(None, &[0, 1, 2, 3], &[
            &markup_escape_text(&commit.summary()),
            &markup_escape_text(&commit.short_id),
            &markup_escape_text(&commit.author),
            &markup_escape_text(&commit.commit_date)
        ]);
    }

    fn add_to_tree(&self, commit: &CommitInfo) {
        self.list_store.insert_with_values(None, &[0, 1, 2, 3], &[
            &commit.summary(),
            &commit.short_id,
            &commit.author,
            &commit.commit_date
        ]);
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
        let root = gtk::ScrolledWindow::new(gtk::NONE_ADJUSTMENT, gtk::NONE_ADJUSTMENT);
        root.set_hexpand(true);
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

    fn refresh_commit_history(&self) {
        self.presenter.update_commit_history();
    }

    fn handle_error(&self, error: impl fmt::Display) {
        self.presenter.parent().view().handle_error(error);
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

        let mut iter = commits.iter();

        if let Some(first_commit) = iter.next() {
            if first_commit.is_sentinel() {
                self.add_to_tree(first_commit);
            } else {
                self.add_to_tree_escaped(first_commit);
            }
        }

        for commit in iter {
            self.add_to_tree_escaped(commit);
        }

        let col = match cursor.1 {
            Some(ref v) => Some(v),
            None => None
        };

        if let Some(path) = cursor.0 {
            self.tree.set_cursor(&path, col, false);
        }

        self.tree.show_all();
    }
}
