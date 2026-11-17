pub mod descriptors;
pub mod device;
pub mod models;
pub mod packet;
pub mod protocol;

pub use descriptors::{
    Descriptor64, Descriptor6E, HandshakeMode, ReportDescriptorInfo, UsbDeviceDescriptors,
};
pub use device::{DiscoveredTablet, TabletDevice};
pub use models::{lookup_pid, pid_quirks, resolve_model_profile, ModelProfile, UGEE_VENDOR_ID};
pub use packet::{ExpressKeyEvent, PacketParserState, PenEvent, SystemEvent, TabletPacket};
pub use protocol::{
    build_android_pass_report, build_battery_poll_report, build_dongle_query_report,
    build_handshake_report, build_led_brightness_report,
};
