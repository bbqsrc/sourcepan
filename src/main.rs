extern crate gtk;
extern crate git2;
extern crate chrono;

use gtk::prelude::*;
use chrono::TimeZone;
use std::env;
use std::cmp::min;

fn main() {
    if gtk::init().is_err() {
        println!("Failed to initialize GTK.");
        return;
    }

    let args: Vec<String> = env::args().map(|x| x).collect();

    let git_repo_path = match args.get(1) {
        Some(v) => v,
        None => {
            println!("Specify path to git repo on the command line.");
            return;
        }
    };

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

    let repo = git2::Repository::open(&git_repo_path).unwrap();
    let mut revwalk = repo.revwalk().unwrap();
    revwalk.push_head().unwrap();

    let oids: Vec<git2::Commit> = revwalk.map(|x| repo.find_commit(x.unwrap()).unwrap()).collect();

    let model = gtk::ListStore::new(&[String::static_type(), String::static_type(), String::static_type(), String::static_type()]);

    for commit in oids {
        let author = commit.author();
        let subid = &format!("{}", commit.id())[0..7];
        let full_summary = commit.summary().unwrap();
        let summary = &format!("{}", &full_summary)[0..min(80, full_summary.len())];

        let naive_dt = chrono::Utc.timestamp(commit.time().seconds(), 0).naive_utc();
        let offset = chrono::offset::FixedOffset::east(commit.time().offset_minutes() * 60);
        let date: chrono::DateTime<chrono::FixedOffset> = chrono::DateTime::from_utc(naive_dt, offset);
        
        model.insert_with_values(None, &[0, 1, 2, 3], &[
            &summary,
            &subid,
            &format!("{} <{}>", author.name().unwrap(), author.email().unwrap()),
            &date.to_string()
        ]);
    }

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

    treeview.set_model(&model);

    let commit_history = gtk::ScrolledWindow::new(None, None);
    commit_history.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    commit_history.add(&treeview);
    let selected_files = gtk::Label::new("Selected files TODO");
    let unstaged_files = gtk::Label::new("Unstaged TODO");
    let diff_view = gtk::Label::new("Diff view TODO");

    let main_pane = gtk::Paned::new(gtk::Orientation::Vertical);
    let file_pane = gtk::Paned::new(gtk::Orientation::Vertical);
    let bottom_pane = gtk::Paned::new(gtk::Orientation::Horizontal);

    file_pane.pack1(&selected_files, true, true);
    file_pane.pack2(&unstaged_files, true, true);

    bottom_pane.pack1(&file_pane, true, true);
    bottom_pane.pack2(&diff_view, true, true);

    main_pane.pack1(&commit_history, true, true);
    main_pane.pack2(&bottom_pane, true, true);

    // stack.add_named(&gtk::Entry::new(), "Branches");
    stack.add_titled(&main_pane, "branch-master", "master");
    window.show_all();

    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(false)
    });

    gtk::main();
}
