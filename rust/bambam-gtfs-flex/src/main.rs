use bambam_gtfs_flex::app::Cli;
use clap::Parser;

fn main() {
    let cli = Cli::parse();
    match cli.run() {
        Ok(()) => {
            log::info!("finished.");
            std::process::exit(0);
        }
        Err(e) => {
            log::error!("{e}");
            std::process::exit(1);
        }
    }
}
