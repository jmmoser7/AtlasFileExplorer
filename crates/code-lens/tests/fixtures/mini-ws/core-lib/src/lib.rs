pub trait Render {
    fn draw(&self);
}

pub struct Point {
    pub x: f32,
    pub y: f32,
}

pub fn origin() -> Point {
    Point { x: 0.0, y: 0.0 }
}

pub mod shapes;
