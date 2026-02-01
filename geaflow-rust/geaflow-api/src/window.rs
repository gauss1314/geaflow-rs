pub enum WindowType {
    Tumbling,
    Sliding,
    Session,
}

pub trait Window {
    fn get_type(&self) -> WindowType;
}

pub struct SizeTumblingWindow {
    pub size: usize,
}

impl Window for SizeTumblingWindow {
    fn get_type(&self) -> WindowType {
        WindowType::Tumbling
    }
}
