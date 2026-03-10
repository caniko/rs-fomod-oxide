use serde::Deserialize;

use crate::error;

/// Metadata from a FOMOD `info.xml` file.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename = "fomod")]
pub struct FomodInfo {
    #[serde(rename = "Name")]
    pub name: Option<String>,

    #[serde(rename = "Author")]
    pub author: Option<String>,

    #[serde(rename = "Version")]
    pub version: Option<String>,

    #[serde(rename = "Description")]
    pub description: Option<String>,

    #[serde(rename = "Website")]
    pub website: Option<String>,

    #[serde(rename = "Id")]
    pub id: Option<String>,
}

impl FomodInfo {
    pub fn parse(xml: &str) -> error::Result<Self> {
        quick_xml::de::from_str(xml).map_err(Into::into)
    }
}
