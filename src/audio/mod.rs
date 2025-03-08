pub mod device;

pub fn devices() -> Vec<device::AudioDevice> {
    device::all()
}

pub fn print_devices() {
    device::print_all()
}