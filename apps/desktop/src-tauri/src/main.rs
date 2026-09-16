// Desktop entry point. Do not add logic here — see lib.rs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    forgeconvert_desktop_lib::run();
}
