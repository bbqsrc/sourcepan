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

#![macro_use]

use std::rc::Rc;

use gtk;

macro_rules! view {
    ($view:expr) => {
        {
            let view = Rc::new($view);
            *view.presenter.view.borrow_mut() = Rc::downgrade(&view);
            view
        }
    };
}

macro_rules! clone {
    (@param _) => ( _ );
    (@param $x:ident) => ( $x );
    ($($n:ident),+ => move || $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move || $body
        }
    );
    ($($n:ident),+ => move |$($p:tt),+| $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move |$(clone!(@param $p),)+| $body
        }
    );
}

macro_rules! weak {
    (@param _) => ( _ );
    (@param $x:ident) => ( $x );
    ($($n:ident),+ => move || $body:expr) => (
        {
            $( let $n = Rc::downgrade(&$n); )+
            move || $body
        }
    );
    ($($n:ident),+ => move |$($p:tt),+| $body:expr) => (
        {
            $( let $n = Rc::downgrade(&$n); )+
            move |$(weak!(@param $p),)+| $body
        }
    );
}

macro_rules! try_upgrade {
    ($n:expr, $e:expr) => {
        match $n.upgrade() {
            Some(v) => v,
            None => return $e
        }
    };

    ($n:expr) => {
        match $n.upgrade() {
            Some(v) => v,
            None => return
        }
    }
}

macro_rules! try_unwrap {
    ($n:expr, $e:expr) => {
        match $n {
            Some(v) => v,
            None => return $e
        }
    };

    ($n:expr) => {
        match $n {
            Some(v) => v,
            None => return
        }
    }
}

use std::fmt;
use gtk::prelude::*;

pub trait AsMessageDialog : fmt::Display {
    fn as_message_dialog<W: IsA<gtk::Window>>(&self, parent: Option<&W>) -> gtk::MessageDialog;
}

impl<T: fmt::Display> AsMessageDialog for T {
    fn as_message_dialog<W: IsA<gtk::Window>>(&self, parent: Option<&W>) -> gtk::MessageDialog {
        let dialog = gtk::MessageDialog::new(
            parent,
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Error,
            gtk::ButtonsType::Close,
            &format!("{}", self)
        );

        dialog.set_title("Error");
        dialog
    }
}

pub trait Window {}

pub trait Parent {
    type View;

    fn parent(&self) -> Option<Rc<Self::View>>;
}

pub mod init;
pub mod main;
