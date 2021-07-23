pub mod utils;

mod data_types;
mod pipeline_builder;
mod recorder;
mod recorder_controller;
mod screencast_portal;
mod settings;
mod timer;

pub use {
    data_types::{Point, Rectangle, Screen, Stream},
    recorder_controller::{RecorderController, RecorderControllerState},
    settings::Settings,
    timer::Timer,
};

use {
    pipeline_builder::PipelineBuilder,
    recorder::{Recorder, RecorderState},
    screencast_portal::ScreencastPortal,
    timer::TimerState,
};
