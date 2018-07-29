use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::fmt;

use git2;
use gtk::prelude::*;
use gtk;

use super::filestatus::{FileStatusView, FileStatusViewable};
use super::history::{HistoryView, HistoryViewable};
use super::CommitInfo;

use ui::main::TreeItem;
use ui::AsMessageDialog;

pub struct BranchPresenter<V> {
    view: RefCell<Weak<V>>,
    repo: Rc<git2::Repository>,
    branch: String
}

impl<V: BranchViewable> BranchPresenter<V> {
    fn new(repo: Rc<git2::Repository>, branch: String) -> BranchPresenter<V> {
        BranchPresenter {
            view: RefCell::new(Weak::new()),
            repo: repo,
            branch: branch
        }
    }

    pub fn branch(&self) -> &str {
        &self.branch
    }

    pub fn repo(&self) -> &git2::Repository {
        &self.repo
    }

    pub fn on_uncommitted_changes_selected(&self) {
        let repo_head_tree = self.repo.head().unwrap().peel_to_tree().unwrap();
        let mut diff_opts = git2::DiffOptions::new();
        diff_opts
            .include_untracked(true)
            .recurse_untracked_dirs(true);

        let mut workdir_diff = self.repo.diff_tree_to_workdir_with_index(Some(&repo_head_tree), Some(&mut diff_opts)).unwrap();
        workdir_diff.find_similar(None).unwrap();

        let mut index_diff = self.repo.diff_tree_to_index(Some(&repo_head_tree), None, None).unwrap();
        index_diff.find_similar(None).unwrap();

        let index_deltas: Vec<TreeItem> = index_diff.deltas().map(|d| {
            TreeItem {
                id: d.new_file().id(),
                path: d.new_file().path().unwrap().to_string_lossy().to_string(),
                delta: d.status()
            }
        }).collect();

        let workdir_deltas: Vec<TreeItem> = workdir_diff.deltas().map(|d| {
            TreeItem {
                id: d.new_file().id(),
                path: d.new_file().path().unwrap().to_string_lossy().to_string(),
                delta: d.status()
            }
        })
        .filter(|x| !index_deltas.iter().any(|y| x.id == y.id))
        .collect();

        self.view().set_staged_statuses(&index_deltas);
        self.view().set_unstaged_statuses(&workdir_deltas);
    }

    pub fn on_commit_selected<'a>(&self, info: &CommitInfo) {
        let commit = self.repo.find_commit(info.id).expect("Commit to exist in repo");
        let parent_commits: Vec<git2::Commit> = commit.parents().collect();

        let maybe_parent = parent_commits.first().map(|x| x.tree().unwrap());
        let parent = match maybe_parent {
            Some(ref v) => Some(v),
            None => None
        };

        let mut diff = self.repo.diff_tree_to_tree(
            parent,
            Some(&commit.tree().unwrap()),
            None
        ).expect("a diff");

        diff.find_similar(None).unwrap();

        let deltas: Vec<TreeItem> = diff.deltas().map(|d| {
            TreeItem {
                id: d.new_file().id(),
                path: d.new_file().path().unwrap().to_string_lossy().to_string(),
                delta: d.status()
            }
        }).collect();

        self.view().set_staged_statuses(&deltas);
        self.view().set_unstaged_statuses(&[]);
    }

    pub fn view(&self) -> Rc<V> {
        self.view.borrow()
            .upgrade()
            .expect("BranchPresenter only running while view still exists")
    }
}

pub trait BranchViewable {
    fn new(window: &gtk::Window, repo: Rc<git2::Repository>, branch: String) -> Rc<Self>;
    fn handle_error(&self, error: impl fmt::Display);
    fn set_staged_statuses(&self, statuses: &[TreeItem]);
    fn set_unstaged_statuses(&self, statuses: &[TreeItem]);
}

#[allow(dead_code)]
pub struct BranchView {
    presenter: Rc<BranchPresenter<BranchView>>,
    history_view: Rc<HistoryView>,
    file_status_view: Rc<FileStatusView>,
    unstaged_view: Rc<FileStatusView>,
    root: gtk::Paned,
    window: gtk::Window
}

impl BranchViewable for BranchView {
    fn new(window: &gtk::Window, repo: Rc<git2::Repository>, branch: String) -> Rc<BranchView> {
        let presenter = Rc::new(BranchPresenter::new(repo, branch));

        let (history_view, file_status_view, unstaged_view, root) = BranchView::create(Rc::downgrade(&presenter));

        let view = view!(BranchView {
            presenter: Rc::clone(&presenter),
            history_view: history_view,
            file_status_view: file_status_view,
            unstaged_view: unstaged_view,
            root: root,
            window: window.clone()
        });

        view
    }
    
    fn handle_error(&self, error: impl fmt::Display) {
        let dialog = error.as_message_dialog(Some(&self.window));
        dialog.run();
        dialog.destroy();
    }
    
    fn set_staged_statuses(&self, statuses: &[TreeItem]) {
        self.file_status_view.presenter.set_history_statuses(statuses);
    }
    
    fn set_unstaged_statuses(&self, statuses: &[TreeItem]) {
        self.unstaged_view.presenter.set_history_statuses(statuses);
    }
}

impl BranchView {
    fn create(parent: Weak<BranchPresenter<BranchView>>) -> (Rc<HistoryView>, Rc<FileStatusView>, Rc<FileStatusView>, gtk::Paned) {
        let commit_history = HistoryView::new(parent);
        let selected_files = FileStatusView::new();
        let unstaged_files = FileStatusView::new();
        let diff_view = gtk::Label::new("Diff view TODO");

        let main_pane = gtk::Paned::new(gtk::Orientation::Vertical);
        let file_pane = gtk::Paned::new(gtk::Orientation::Vertical);
        let bottom_pane = gtk::Paned::new(gtk::Orientation::Horizontal);

        // Add everything to the panes
        file_pane.pack1(selected_files.widget(), true, true);
        file_pane.pack2(unstaged_files.widget(), true, true);

        bottom_pane.pack1(&file_pane, true, true);
        bottom_pane.pack2(&diff_view, true, true);

        main_pane.pack1(commit_history.widget(), true, true);
        main_pane.pack2(&bottom_pane, true, true);

        (commit_history, selected_files, unstaged_files, main_pane)
    }

    pub fn widget(&self) -> &gtk::Paned {
        &self.root
    }
}
