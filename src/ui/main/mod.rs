mod branch;

use std::rc::{Rc, Weak};
use std::cell::RefCell;

use git2;
use gtk::prelude::*;
use gtk;

use ui::main::branch::{BranchViewable, BranchView};

struct MainPresenter<V> {
    view: RefCell<Weak<V>>,
    repo: Rc<git2::Repository>
}

pub trait MainViewable {
    fn with_repo(repo: git2::Repository) -> Rc<Self>;
    fn show(&self);
}

impl<V: MainViewable> MainPresenter<V> {
    fn new(repo: git2::Repository) -> MainPresenter<V> {
        MainPresenter {
            view: RefCell::new(Weak::new()),
            repo: Rc::new(repo)
        }
    }
}

impl MainViewable for MainWindow {
    fn with_repo(repo: git2::Repository) -> Rc<Self> {
        let (window, stack) = MainWindow::create();


        let view = Rc::new(MainWindow {
            presenter: MainPresenter::new(repo),
            sidebar_stack: stack,
            window: window
        });

        // TODO move
        {
            let repo = view.presenter.repo.clone();
            for entry in repo.branches(Some(git2::BranchType::Local)).unwrap().into_iter() {
                let (branch, _) = entry.unwrap();
                let name = branch.name().unwrap().unwrap();
                let branch_view = BranchView::new(view.presenter.repo.clone(), name.to_string());
                &view.sidebar_stack.add_titled(branch_view.widget(), &format!("branch-{}", name), name);
            }
        }

        *view.presenter.view.borrow_mut() = Rc::downgrade(&view);

        view
    }

    fn show(&self) {
        self.window.show_all();
    }
}

pub struct MainWindow {
    presenter: MainPresenter<MainWindow>,
    sidebar_stack: gtk::Stack,
    window: gtk::Window
}

impl MainWindow {
    fn create() -> (gtk::Window, gtk::Stack) {
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("Sourcepan");
        window.set_default_size(1024, 768);

        let header_bar = gtk::HeaderBar::new();
        header_bar.set_title("Sourcepan");
        header_bar.set_show_close_button(true);

        let fetch_button = gtk::Button::new_with_label("Fetch");
        let settings_button = gtk::Button::new_with_label("Preferences");
        header_bar.pack_end(&settings_button);
        header_bar.pack_start(&fetch_button);

        window.set_titlebar(&header_bar);

        let main_box = gtk::Grid::new();

        window.add(&main_box);
        
        let sidebar = gtk::StackSidebar::new();
        main_box.attach(&sidebar, 0, 0, 1, 1);

        let stack = gtk::Stack::new();
        stack.set_vexpand(true);
        stack.set_hexpand(true);
        sidebar.set_stack(&stack);
        main_box.attach(&stack, 1, 0, 1, 1);

        window.connect_delete_event(|_, _| {
            gtk::main_quit();
            Inhibit(false)
        });

        (window, stack)
    }
}