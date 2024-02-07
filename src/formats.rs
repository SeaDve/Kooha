use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use gtk::{gio, glib};
use serde::Deserialize;
use serde_yaml::Value;

#[derive(Debug, Deserialize)]
struct Document {
    formats: Vec<Format>,
    experimental_formats: Vec<Format>,
}

#[derive(Debug, Deserialize)]
struct Format {
    id: String,
    name: String,
    file_extension: String,
    video_encoder: Value,
    audio_encoder: Value,
    container: Value,
}

struct Element {
    name: String,
    properties: HashMap<String, glib::SendValue>,
    caps_field: HashMap<String, glib::SendValue>,
}

impl Element {
    fn from_value(value: Value) -> Result<Self> {
        match value {
            Value::String(name) => Ok(Self {
                name,
                properties: HashMap::new(),
                caps_field: HashMap::new(),
            }),
            Value::Mapping(mut mapping) => {
                // Remove caps_field first.
                let caps_field = match mapping.remove("caps_fields") {
                    Some(Value::Mapping(caps_field)) => caps_field
                        .into_iter()
                        .map(|(k, v)| {
                            match k {
                                Value::String(k) => Ok((k, v)),
                                _ => bail!("Invalid caps_field key"),
                            }
                            Ok(())
                        })
                        .collect::<Result<HashMap<_, _>>>()?,
                    None => HashMap::new(),
                    _ => bail!("Invalid caps_field value"),
                };

                let (name, properties) = mapping.into_iter().next().context("No name found")?;

                let name = match name {
                    Value::String(name) => name,
                    _ => bail!("Invalid name value"),
                };

                let properties = match properties {
                    Value::Mapping(properties) => properties,
                    _ => bail!("Invalid properties value"),
                };

                todo!()
            }
            _ => bail!("Invalid encoder value"),
        }
    }
}

pub fn get() -> Result<()> {
    let data = gio::resources_lookup_data(
        "/io/github/seadve/Kooha/formats.yml",
        gio::ResourceLookupFlags::NONE,
    )?;

    dbg!(serde_yaml::from_slice::<Document>(&data).unwrap());

    Ok(())
}
