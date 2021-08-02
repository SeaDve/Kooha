use gtk::glib::{self, GBoxed};

use std::mem;

#[derive(Debug, Clone, GBoxed)]
#[gboxed(type_name = "Rectangle")]
pub struct Rectangle {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rectangle {
    pub fn from_points(point_1: Point, point_2: Point) -> Self {
        let mut x = point_1.x.min(point_2.x);
        let mut y = point_1.y.min(point_2.y);
        let mut width = (point_1.x - point_2.x).abs();
        let mut height = (point_1.y - point_2.y).abs();

        if width == 0.0 && height == 0.0 {
            mem::swap(&mut width, &mut x);
            mem::swap(&mut height, &mut y);
        }

        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn rescale(mut self, scale_factor: f64) -> Self {
        self.x *= scale_factor;
        self.y *= scale_factor;
        self.width *= scale_factor;
        self.height *= scale_factor;
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Default, Clone, GBoxed)]
#[gboxed(type_name = "Screen")]
pub struct Screen {
    pub width: i32,
    pub height: i32,
}

impl Screen {
    pub fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Default, Clone, GBoxed)]
#[gboxed(type_name = "Stream")]
pub struct Stream {
    pub fd: i32,
    pub node_id: u32,
    pub screen: Screen,
}
