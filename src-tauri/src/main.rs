// Évite une fenêtre console supplémentaire sous Windows en release. NE PAS SUPPRIMER.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    mmc_batch_vf_lib::run()
}
