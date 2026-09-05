// Empêche une console de s'ouvrir sous Windows en version finale.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() { montis_lib::run() }
