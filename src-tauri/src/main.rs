//! BXNode Bot Desktop Entry Point

// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    bxnode_bot_desktop::run()
}
