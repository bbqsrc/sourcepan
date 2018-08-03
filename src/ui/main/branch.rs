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
use std::fmt;

use git2;
use gtk::prelude::*;
use gtk;

use super::filestatus::{FileStatusView, FileStatusViewable};
use super::history::{HistoryView, HistoryViewable};
use super::diff::DiffView;
use super::CommitInfo;

use ui::main::TreeItem;
use ui::AsMessageDialog;

pub struct BranchPresenter<V> {
    view: RefCell<Weak<V>>,
    repo: RefCell<Rc<git2::Repository>>,
    deltas: RefCell<(Vec<TreeItem>, Vec<TreeItem>)>,
    branch: RefCell<String>
}

impl<V: BranchViewable> BranchPresenter<V> {
    fn new(repo: Rc<git2::Repository>, initial_branch: String) -> BranchPresenter<V> {
         BranchPresenter {
            view: RefCell::new(Weak::new()),
            repo: RefCell::new(repo),
            deltas: RefCell::new((vec![], vec![])),
            branch: RefCell::new(initial_branch)
        }
    }

    pub fn repo(&self) -> Rc<git2::Repository> {
        self.repo.borrow().clone()
    }

    pub fn deltas(&self) -> &RefCell<(Vec<TreeItem>, Vec<TreeItem>)> {
        &self.deltas
    }

    pub fn branch(&self) -> &RefCell<String> {
        &self.branch
    }

    pub fn set_branch(&self, branch: &str) {
        *self.branch.borrow_mut() = branch.to_string();

        self.view().refresh_commit_history();
    }

    pub fn set_repo(&self, repo: Rc<git2::Repository>) {
        let branch = {
            let repo = repo.clone();
            let first_branch = repo.branches(None).unwrap()
                .next().unwrap().unwrap();
            first_branch.0.name().unwrap().unwrap()
                .to_string()
        };

        *self.branch.borrow_mut() = branch;
        *self.repo.borrow_mut() = repo;

        self.view().refresh_commit_history();
    }

    pub fn on_uncommitted_changes_selected(&self) {
        let repo = self.repo.borrow();
        let repo_head_tree = repo.head().unwrap().peel_to_tree().unwrap();
        let mut diff_opts = git2::DiffOptions::new();
        diff_opts
            .include_untracked(true)
            .recurse_untracked_dirs(true);

        let mut workdir_diff = repo.diff_tree_to_workdir_with_index(Some(&repo_head_tree), Some(&mut diff_opts)).unwrap();
        workdir_diff.find_similar(None).unwrap();

        let mut index_diff = repo.diff_tree_to_index(Some(&repo_head_tree), None, None).unwrap();
        index_diff.find_similar(None).unwrap();

        let index_deltas: Vec<TreeItem> = index_diff.deltas().map(|d| {
            TreeItem {
                id: d.new_file().id(),
                path: d.new_file().path().unwrap().to_string_lossy().to_string(),
                delta: d.status(),
                is_selected: true
            }
        }).collect();

        let workdir_deltas: Vec<TreeItem> = workdir_diff.deltas().map(|d| {
            TreeItem {
                id: d.new_file().id(),
                path: d.new_file().path().unwrap().to_string_lossy().to_string(),
                delta: d.status(),
                is_selected: false
            }
        })
        .filter(|x| !index_deltas.iter().any(|y| x.id == y.id))
        .collect();

        self.view().set_statuses(&index_deltas, &workdir_deltas);
        self.view().set_diff(index_diff);

        *self.deltas.borrow_mut() = (index_deltas, workdir_deltas);
    }

    pub fn on_commit_selected<'a>(&self, info: &CommitInfo) {
        let repo = self.repo.borrow();
        let commit = repo.find_commit(info.id).expect("Commit to exist in repo");
        let parent_commits: Vec<git2::Commit> = commit.parents().collect();

        let maybe_parent = parent_commits.first().map(|x| x.tree().unwrap());
        let parent = match maybe_parent {
            Some(ref v) => Some(v),
            None => None
        };

        let mut diff = repo.diff_tree_to_tree(
            parent,
            Some(&commit.tree().unwrap()),
            None
        ).expect("a diff");

        diff.find_similar(None).unwrap();

        let deltas: Vec<TreeItem> = diff.deltas().map(|d| {
            TreeItem {
                id: d.new_file().id(),
                path: d.new_file().path().unwrap().to_string_lossy().to_string(),
                delta: d.status(),
                is_selected: true
            }
        }).collect();

        self.view().set_overview_statuses(&deltas, &commit);
        self.view().set_diff(diff);
    }

    pub fn view(&self) -> Rc<V> {
        self.view.borrow()
            .upgrade()
            .expect("BranchPresenter only running while view still exists")
    }
}

pub trait BranchViewable {
    fn handle_error(&self, error: impl fmt::Display);
    fn set_overview_statuses(&self, statuses: &[TreeItem], commit: &git2::Commit);
    fn set_statuses(&self, staged: &[TreeItem], unstaged: &[TreeItem]);
    fn set_diff(&self, diff: git2::Diff);
    fn set_repo(&self, repo: Rc<git2::Repository>);
    fn set_branch(&self, branch: &str);
    fn refresh_commit_history(&self);
}

#[allow(dead_code)]
pub struct BranchView {
    presenter: Rc<BranchPresenter<BranchView>>,
    history_view: Rc<HistoryView>,
    files_view: Rc<FileStatusView>,
    diff_view: DiffView,
    root: gtk::Paned,
    window: gtk::Window
}

impl BranchViewable for BranchView {
    fn set_branch(&self, branch: &str) {
        self.presenter.set_branch(branch);
    }

    fn set_repo(&self, repo: Rc<git2::Repository>) {
        self.presenter.set_repo(repo);
    }

    fn refresh_commit_history(&self) {
        self.history_view.refresh_commit_history();
    }
    
    fn handle_error(&self, error: impl fmt::Display) {
        let dialog = error.as_message_dialog(Some(&self.window));
        dialog.run();
        dialog.destroy();
    }
    
    fn set_statuses(&self, staged: &[TreeItem], unstaged: &[TreeItem]) {
        self.files_view.presenter.set_history_statuses(staged, unstaged);
    }

    fn set_overview_statuses(&self, statuses: &[TreeItem], commit: &git2::Commit) {
        self.files_view.presenter.set_overview_statuses(statuses, commit);
    }

    fn set_diff(&self, diff: git2::Diff) {
        self.diff_view.set_diff(diff);
    }
}

impl BranchView {
    pub fn new(window: &gtk::Window, repo: Rc<git2::Repository>, initial_branch: String) -> Rc<BranchView> {
        let presenter = Rc::new(BranchPresenter::new(repo, initial_branch));

        let (history_view, files_view, diff_view, root) = BranchView::create(Rc::downgrade(&presenter));

        let view = view!(BranchView {
            presenter: Rc::clone(&presenter),
            history_view, 
            files_view,
            diff_view,
            root: root,
            window: window.clone()
        });

        view
    }

    fn create(parent: Weak<BranchPresenter<BranchView>>) -> (Rc<HistoryView>, Rc<FileStatusView>, DiffView, gtk::Paned) {
        let commit_history = HistoryView::new(parent.clone());
        let files_view = FileStatusView::new(parent.clone());
        let diff_view = DiffView::new();

        let main_pane = gtk::Paned::new(gtk::Orientation::Vertical);
        let bottom_pane = gtk::Paned::new(gtk::Orientation::Horizontal);

        bottom_pane.pack1(files_view.widget(), true, true);
        bottom_pane.pack2(diff_view.widget(), true, true);

        main_pane.pack1(commit_history.widget(), true, true);
        main_pane.pack2(&bottom_pane, true, true);

        (commit_history, files_view, diff_view, main_pane)
    }

    pub fn widget(&self) -> &gtk::Paned {
        &self.root
    }

    pub fn set_file_pane_to_half(&self) {
        let height = self.files_view.widget().get_allocated_height();
        self.files_view.widget().set_position(height / 9 * 4);
    }
}
