use serde::{Deserialize, Serialize};
use zbus::{
    proxy,
    zvariant::{OwnedValue, Str, Structure, Type},
};

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Type, Default)]
pub enum StatusNotifierStatus {
    #[default]
    Passive,
    Active,
    NeedsAttention,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Type)]
pub enum StatusNotifierCategory {
    ApplicationStatus,
    Communications,
    SystemServices,
    Hardware,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StatusNotifierTooltip {
    pub icon_name: String,
    pub icon_pixmap: Vec<(i32, i32, Vec<u8>)>,
    pub title: String,
    pub description: String,
}

impl TryFrom<OwnedValue> for StatusNotifierTooltip {
    type Error = zbus::zvariant::Error;

    fn try_from(value: OwnedValue) -> Result<Self, Self::Error> {
        let structure: Structure = value.downcast_ref()?;
        let (icon_name, icon_pixmap, title, description) = structure.try_into()?;
        Ok(Self {
            icon_name,
            icon_pixmap,
            title,
            description,
        })
    }
}

/// Proxy trait for StatusNotifierItem D-Bus interface
#[proxy(
    interface = "org.freedesktop.StatusNotifierItem",
    default_path = "/StatusNotifierItem"
)]
pub trait StatusNotifierItem {
    #[zbus(property)]
    fn category(&self) -> zbus::Result<StatusNotifierCategory>;

    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn title(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn status(&self) -> zbus::Result<StatusNotifierStatus>;

    #[zbus(property)]
    fn window_id(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn icon_name(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn icon_pixmap(&self) -> zbus::Result<(i32, i32, Vec<u8>)>;

    #[zbus(property)]
    fn overlay_icon_name(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> zbus::Result<Vec<(i32, i32, Vec<u8>)>>;

    #[zbus(property)]
    fn attention_icon_name(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> zbus::Result<Vec<(i32, i32, Vec<u8>)>>;

    #[zbus(property)]
    fn attention_movie_name(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn tool_tip(&self) -> zbus::Result<StatusNotifierTooltip>;

    #[zbus(property)]
    fn menu(&self) -> zbus::Result<String>;

    fn context_menu(&self, x: i32, y: i32) -> zbus::Result<()>;

    fn activate(&self, x: i32, y: i32) -> zbus::Result<()>;

    fn secondary_activate(&self, x: i32, y: i32) -> zbus::Result<()>;

    fn scroll(&self, delta: i32, orientation: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn new_title(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn new_icon(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn new_attention_icon(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn new_overlay_icon(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn new_tool_tip(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn new_status(&self, status: String) -> zbus::Result<()>;
}

impl TryFrom<OwnedValue> for StatusNotifierStatus {
    type Error = zbus::zvariant::Error;

    fn try_from(value: OwnedValue) -> Result<Self, Self::Error> {
        let str: Str = value.downcast_ref()?;
        Ok(Self::from(str))
    }
}

impl From<Str<'_>> for StatusNotifierStatus {
    fn from(s: Str) -> Self {
        match s.as_str() {
            "Active" => StatusNotifierStatus::Active,
            "NeedsAttention" => StatusNotifierStatus::NeedsAttention,
            _ => StatusNotifierStatus::Passive,
        }
    }
}

impl TryFrom<OwnedValue> for StatusNotifierCategory {
    type Error = zbus::zvariant::Error;

    fn try_from(value: OwnedValue) -> Result<Self, Self::Error> {
        let str: Str = value.downcast_ref()?;
        Ok(Self::from(str))
    }
}

impl From<Str<'_>> for StatusNotifierCategory {
    fn from(s: Str) -> Self {
        match s.as_str() {
            "ApplicationStatus" => Self::ApplicationStatus,
            "Communications" => Self::Communications,
            "SystemServices" => Self::SystemServices,
            "Hardware" => Self::Hardware,
            _ => Self::Other,
        }
    }
}
