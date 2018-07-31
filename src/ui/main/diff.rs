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

use std::cell::RefCell;

use git2;
use gtk::prelude::*;
use gtk;
use gdk;
use pango;

const NO_NL_STR: &'static str = "No newline at end of file";

trait DiffViewable {

}

trait DiffChunkViewable {

}

#[allow(dead_code)]
pub struct DiffChunkView {
    list_store: gtk::ListStore,
    label: gtk::Label,
    lines_tree: gtk::TreeView,
    count_tree: gtk::TreeView,
    root: gtk::Box
}

impl DiffChunkViewable for DiffChunkView {

}

pub struct DiffView {
    root: gtk::ScrolledWindow,
    container: gtk::Box,
    files: RefCell<Vec<DiffFileView>>
}

impl DiffView {
    pub fn new() -> DiffView {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
        container.get_style_context().unwrap().add_class("diff-container");

        let root = gtk::ScrolledWindow::new(None, None);
        root.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        root.add(&container);

        DiffView {
            root,
            container,
            files: RefCell::new(vec![])
        }
    }

    pub fn set_diff(&self, diff: git2::Diff) {
        for child in self.container.get_children() {
            self.container.remove(&child);
        }

        let mut views = vec![];

        for (n, d) in diff.deltas().enumerate() {
            let mut patch = git2::Patch::from_diff(&diff, n).unwrap().unwrap();
            let path = d.new_file().path().unwrap().to_string_lossy();

            let view = DiffFileView::new(patch, &path);
            self.container.add(view.widget());
            views.push(view);
        }

        *self.files.borrow_mut() = views;

        self.container.show_all();
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.root
    }
}

#[allow(dead_code)]
struct DiffFileView {
    root: gtk::Box,
    label: gtk::Label
}

impl DiffFileView {
    pub fn new(patch: git2::Patch, path: &str) -> DiffFileView {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.get_style_context().unwrap().add_class("file-border");
        
        let label = gtk::Label::new(path);
        label.set_xalign(0.0);
        label.set_ellipsize(pango::EllipsizeMode::Middle);
        label.get_style_context().unwrap().add_class("file-label");

        root.add(&label);

        for hunk_idx in 0..patch.num_hunks() {
            let hunk = patch.hunk(hunk_idx).unwrap().0;
            let header = ::std::str::from_utf8(hunk.header()).unwrap();

            let chunk_view = DiffChunkView::new(&patch, header, hunk_idx);
            root.add(chunk_view.widget());
        }

        DiffFileView {
            root,
            label
        }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}

trait HumanDiffLineExt<'a> {
    fn origin_human(&self) -> char;
    fn content_str(&self) -> Option<&str>;
}

impl<'a> HumanDiffLineExt<'a> for git2::DiffLine<'a> {
    fn origin_human(&self) -> char {
        let ch = self.origin();
        match ch {
            '=' | '>' | '<' => '\\',
            _ => ch
        }
    }

    fn content_str(&self) -> Option<&str> {
        match ::std::str::from_utf8(self.content()) {
            Ok(v) => {
                // Check for weird NL string
                if v.trim_right().ends_with(NO_NL_STR) {
                    Some(NO_NL_STR)
                } else {
                    Some(v.trim_right())
                }
            },
            Err(_) => None
        }
    }
                
}

impl DiffChunkView {
    pub fn new(patch: &git2::Patch, header: &str, hunk_idx: usize) -> DiffChunkView {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.get_style_context().unwrap().add_class("tree-border");

        let label = gtk::Label::new(header.trim());
        label.set_xalign(0.0);
        label.get_style_context().unwrap().add_class("diff-label");
        label.set_ellipsize(pango::EllipsizeMode::Middle);

        let count_tree = gtk::TreeView::new();
        count_tree.get_style_context().unwrap().add_class("line-count");
        count_tree.get_selection().set_mode(gtk::SelectionMode::None);

        let lines_tree = gtk::TreeView::new();
        lines_tree.get_style_context().unwrap().add_class("monospace");
        lines_tree.get_selection().set_mode(gtk::SelectionMode::Multiple);

        let list_store = gtk::ListStore::new(&[
            gdk::RGBA::static_type(),
            String::static_type(),
            String::static_type(),
            String::static_type(),
            String::static_type(),
            gdk::RGBA::static_type()
        ]);

        let trees_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        trees_box.add(&count_tree);

        let scroller = gtk::ScrolledWindow::new(None, None);
        scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
        scroller.add(&lines_tree);
        scroller.set_hexpand(true);
        trees_box.add(&scroller);

        root.add(&label);
        root.add(&trees_box);

        fn append_column(tree: &gtk::TreeView, id: i32, title: &str, is_colored: bool) -> gtk::TreeViewColumn {
            let column = gtk::TreeViewColumn::new();
            let cell = gtk::CellRendererText::new();

            if id <= 2 {
                cell.set_alignment(0.5, 0.5);
            }

            column.pack_start(&cell, true);
            column.set_resizable(true);
            column.set_title(title);
            column.add_attribute(&cell, "text", id);
            if is_colored {
                column.add_attribute(&cell, "background-rgba", 0);
            } else {
                column.add_attribute(&cell, "background-rgba", 5);
                column.set_min_width(24);
            }
            tree.append_column(&column);

            column
        }
        
        let column = gtk::TreeViewColumn::new();
        let cell = gtk::CellRendererText::new();
        column.pack_start(&cell, true);
        column.set_visible(false);
        
        append_column(&count_tree, 1, "Old", false);
        append_column(&count_tree, 2, "New", false);
        append_column(&lines_tree, 3, "Origin", true);
        append_column(&lines_tree, 4, "Line", true);

        count_tree.set_headers_visible(false);
        lines_tree.set_headers_visible(false);
        
        count_tree.set_model(&list_store);
        lines_tree.set_model(&list_store);

        for i in 0..patch.num_lines_in_hunk(hunk_idx).unwrap() {
            let line = patch.line_in_hunk(hunk_idx, i).unwrap();

            let rgba = match line.origin() {
                '<' | '+' => gdk::RGBA {
                    red: 0.851,
                    green: 0.925,
                    blue: 0.812,
                    alpha: 1.0
                },
                '>' | '-' => gdk::RGBA {
                    red: 0.918,
                    green: 0.835,
                    blue: 0.835,
                    alpha: 1.0
                },
                _ => gdk::RGBA::white()
            };

            (&list_store).insert_with_values(None, &[0, 1, 2, 3, 4, 5], &[
                &rgba,
                &line.old_lineno().map(|x| x.to_string()).unwrap_or("".to_string()),
                &line.new_lineno().map(|x| x.to_string()).unwrap_or("".to_string()),
                &line.origin_human().to_string(),
                &line.content_str().unwrap_or("<unknown>"),
                &gdk::RGBA { red: 0.95, green: 0.95, blue: 0.95, alpha: 1.0 }
            ]);
        }
        
        root.show_all();

        DiffChunkView {
            list_store,
            label,
            count_tree,
            lines_tree,
            root
        }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}
