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
    repo: Rc<git2::Repository>,
    deltas: RefCell<(Vec<TreeItem>, Vec<TreeItem>)>,
    branch: String
}

impl<V: BranchViewable> BranchPresenter<V> {
    fn new(repo: Rc<git2::Repository>, branch: String) -> BranchPresenter<V> {
        BranchPresenter {
            view: RefCell::new(Weak::new()),
            repo: repo,
            deltas: RefCell::new((vec![], vec![])),
            branch: branch
        }
    }

    pub fn branch(&self) -> &str {
        &self.branch
    }

    pub fn repo(&self) -> &git2::Repository {
        &self.repo
    }

    pub fn deltas(&self) -> &RefCell<(Vec<TreeItem>, Vec<TreeItem>)> {
        &self.deltas
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

        *self.deltas.borrow_mut() = (index_deltas, workdir_deltas);
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
    fn new(window: &gtk::Window, repo: Rc<git2::Repository>, branch: String) -> Rc<Self>;
    fn handle_error(&self, error: impl fmt::Display);
    fn set_overview_statuses(&self, statuses: &[TreeItem], commit: &git2::Commit);
    fn set_statuses(&self, staged: &[TreeItem], unstaged: &[TreeItem]);
    fn set_diff(&self, diff: git2::Diff);
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
    fn new(window: &gtk::Window, repo: Rc<git2::Repository>, branch: String) -> Rc<BranchView> {
        let presenter = Rc::new(BranchPresenter::new(repo, branch));

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
}
