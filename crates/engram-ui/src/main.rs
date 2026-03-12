use leptos::prelude::*;

mod api;
mod app;
mod auth;
mod components;
mod pages;
mod utils;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(app::App);
}
