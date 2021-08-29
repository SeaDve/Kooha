use std::mem;

#[derive(Debug, Clone, Copy)]
pub struct Rectangle {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rectangle {
    pub fn from_points(point_1: &Point, point_2: &Point) -> Self {
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

#[derive(Debug)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Default)]
pub struct Screen {
    pub width: i32,
    pub height: i32,
}

impl Screen {
    pub fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use approx::{assert_relative_eq, assert_ulps_eq};

    #[test]
    fn rectangle_default() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(0.0, 0.0);
        let rect = Rectangle::from_points(&p1, &p2);

        assert_relative_eq!(rect.x, 0.0);
        assert_relative_eq!(rect.y, 0.0);
        assert_relative_eq!(rect.width, 0.0);
        assert_relative_eq!(rect.height, 0.0);
    }

    #[test]
    fn rectangle_smaller_p1() {
        let p1 = Point::new(5.0, 6.0);
        let p2 = Point::new(10.0, 13.0);
        let rect = Rectangle::from_points(&p1, &p2);

        assert_relative_eq!(rect.x, 5.0);
        assert_relative_eq!(rect.y, 6.0);
        assert_relative_eq!(rect.width, 5.0);
        assert_relative_eq!(rect.height, 7.0);
    }

    #[test]
    fn rectangle_smaller_p2() {
        let p1 = Point::new(10.0, 13.0);
        let p2 = Point::new(5.0, 6.0);
        let rect = Rectangle::from_points(&p1, &p2);

        assert_relative_eq!(rect.x, 5.0);
        assert_relative_eq!(rect.y, 6.0);
        assert_relative_eq!(rect.width, 5.0);
        assert_relative_eq!(rect.height, 7.0);
    }

    #[test]
    fn rectangle_mixed_smaller_1() {
        let p1 = Point::new(10.0, 6.0);
        let p2 = Point::new(5.0, 13.0);
        let rect = Rectangle::from_points(&p1, &p2);

        assert_relative_eq!(rect.x, 5.0);
        assert_relative_eq!(rect.y, 6.0);
        assert_relative_eq!(rect.width, 5.0);
        assert_relative_eq!(rect.height, 7.0);
    }

    #[test]
    fn rectangle_mixed_smaller_2() {
        let p1 = Point::new(5.0, 10.0);
        let p2 = Point::new(13.0, 6.0);
        let rect = Rectangle::from_points(&p1, &p2);

        assert_relative_eq!(rect.x, 5.0);
        assert_relative_eq!(rect.y, 6.0);
        assert_relative_eq!(rect.width, 8.0);
        assert_relative_eq!(rect.height, 4.0);
    }

    #[test]
    fn rectangle_rescale() {
        let p1 = Point::new(8.1, 13.7);
        let p2 = Point::new(14.3, 11.3);
        let rect = Rectangle::from_points(&p1, &p2);
        let rescaled_rect = rect.rescale(5.0);

        assert_relative_eq!(rescaled_rect.x, 8.1 * 5.0);
        assert_relative_eq!(rescaled_rect.y, 11.3 * 5.0);
        assert_ulps_eq!(rescaled_rect.width, 6.2 * 5.0);
        assert_ulps_eq!(rescaled_rect.height, 2.4 * 5.0);
    }
}
