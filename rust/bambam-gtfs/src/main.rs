//! this script reads all GTFS archives in the MobilityData catalog and collects information
//! about the trips, WKT shapes, and trip legs of the collection.
use bambam_gtfs::schedule::app::GtfsApp;
use clap::Parser;

fn main() {
    env_logger::init();
    let args = GtfsApp::parse();
    args.op.run()
}
