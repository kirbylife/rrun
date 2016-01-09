#![crate_type = "bin"]
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate gtk;
extern crate gdk;
extern crate toml;
extern crate itertools;
extern crate regex;

use gtk::traits::*;
use std::rc::Rc;
use std::io::prelude::*;
use std::fs;
use std::fs::File;
use std::error::Error;
use std::path::Path;
use std::env;
use itertools::Itertools;
use std::cell::Cell;
use std::sync::{Arc, Mutex};
use gtk::signal::Inhibit;
use gdk::enums::key;
use gdk::enums::modifier_type;
use autocomplete::Completion;
use engine::DefaultEngine;
use engine::Engine;
mod engine;
mod autocomplete;
mod externalautocompleter;
mod runner;
mod externalrunner;
mod execution;

#[macro_export]
macro_rules! trys {($e: expr) => {match $e {
    Ok (ok) => ok,
    Err (err) => {
        return Err (format! ("{}:{}] {}", file!(), line!(), err));
        }
    }
}
}

#[cfg(feature="search_entry")]
fn get_entry_field() -> gtk::SearchEntry {
    gtk::SearchEntry::new().unwrap()
}

#[cfg(not(feature="search_entry"))]
fn get_entry_field() -> gtk::Entry {
    gtk::Entry::new().unwrap()
}

fn get_config_file() -> Result<File, String> {
    // Create a path to the desired file
    let config_directory = match env::home_dir() {
        Some(dir) => dir.join(Path::new(".config/rrun")),
        None => panic!("Unable to get $HOME")
    };
    if fs::create_dir_all(&config_directory).is_err() {
        panic!("Unable to create config directory {:?}", config_directory);
    };
    let config_path = config_directory.join(Path::new("config.toml"));

    match File::open(&config_path) {
        Err(why) => {
            info!("couldn't open {}: {}", config_path.display(), Error::description(&why));
            println!("Creating initial config file in ~/.config/rrun/config.toml.");
            let mut f = trys!(File::create(&config_path));
            trys!(f.write_all(include_str!("config.toml").as_bytes()));
            trys!(f.flush());
            drop(f);
            Ok(trys!(File::open(&config_path)))
        },
        Ok(file) => Ok(file),
    }
}

fn read_config(config_file: &mut File) -> toml::Table {
    let mut toml = String::new();
    match config_file.read_to_string(&mut toml) {
        Err(why) => panic!("couldn't read Configfile ~/.config/rrun/config.toml: {}",
                                                   Error::description(&why)),
        Ok(_) => (),
    }

    let config = toml::Parser::new(&toml).parse().unwrap();
    debug!("config.toml contains the following configuration\n{:?}", config);
    config
}

#[allow(dead_code)]
fn main() {
    let mut file = get_config_file().unwrap();
    let config = read_config(&mut file);
    let engine = DefaultEngine::new(&config);

    gtk::init().unwrap_or_else(|_| panic!("Failed to initialize GTK."));
    debug!("Major: {}, Minor: {}", gtk::get_major_version(), gtk::get_minor_version());

    let last_pressed_key: Rc<Cell<i32>> = Rc::new(Cell::new(0));

    let window = gtk::Window::new(gtk::WindowType::Toplevel).unwrap();
    env_logger::init().unwrap();
    let entry = get_entry_field();

    window.set_title("rrun");
    window.set_window_position(gtk::WindowPosition::Center);

    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(true)
    });

    window.set_decorated(false);
    window.add(&entry);
    window.set_border_width(0);
    window.show_all();
    let the_completions: Arc<Mutex<Box<Iterator<Item = Completion>>>> = Arc::new(Mutex::new(Box::new(vec![].into_iter())));
    let the_current_completion: Arc<Mutex<Box<Option<Completion>>>> = Arc::new(Mutex::new(Box::new(None)));
    window.connect_key_press_event(move |_, key| {
        let completions = the_completions.clone();
        let current_completion = the_current_completion.clone();
        let keyval = key.keyval as i32;
        let keystate = (*key).state;
        debug!("key pressed: {}", keyval);
        match keyval {
            key::Escape => gtk::main_quit(),
            key::Return => {
                debug!("keystate: {:?}", keystate);
                debug!("Controlmask == {:?}", modifier_type::ControlMask);
                let query = entry.get_text().unwrap();
                let comp = *current_completion.lock().unwrap().clone();
                let the_completion = comp.unwrap_or(engine.get_completions(&query).next().unwrap().to_owned());
                if keystate.intersects(modifier_type::ControlMask) {
                    let output = engine.run_completion(&the_completion, false).unwrap();
                    debug!("ctrl pressed!");
                    if output.len() > 0 {
                        entry.set_text(output.trim());
                        entry.set_position(-1);
                    }
                } else {
                    let _ = engine.run_completion(&the_completion, true);
                    gtk::main_quit();
                }

            }
            key::Tab => {
                if last_pressed_key.get() != key::Tab {
                    let query = &entry.get_text().unwrap();
                    let current_completions = engine.get_completions(query);
                    *completions.lock().unwrap() = current_completions;
                }
                let new_completion = completions.lock().unwrap().next();
                debug!("new_completion: {:?}", new_completion);

                if new_completion.is_some() {
                    *current_completion.lock().unwrap() = Box::new(new_completion.clone());
                    entry.set_text(new_completion.unwrap().text.trim());
                    entry.set_position(-1);
                    last_pressed_key.set(key::Tab);
                    return Inhibit(true);
                }
            }
            _ => (),
        }
        last_pressed_key.set((*key).keyval as i32);
        Inhibit(false)
    });

    gtk::main();
}
