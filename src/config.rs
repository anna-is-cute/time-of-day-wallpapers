use std::path::PathBuf;

use serde::{de::{value::StrDeserializer, Visitor}, Deserialize};

use crate::Light;

#[derive(Deserialize)]
pub struct Config {
    pub location: Location,
    pub method: Method,
    #[serde(rename = "wallpaper")]
    pub wallpapers: Vec<Wallpaper>,
}

#[derive(Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Deserialize)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum Method {
    Kde,
}

#[derive(Deserialize)]
pub struct Wallpaper {
    pub during: During,
    pub path: PathBuf,
}

pub enum During {
    Single(Light),
    Multiple(Vec<Light>),
    Any,
}

impl During {
    pub fn is_any(&self) -> bool {
        return matches!(self, Self::Any);
    }
}

impl<'de> Deserialize<'de> for During {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(DuringVisitor)
    }
}

struct DuringVisitor;

impl<'de> Visitor<'de> for DuringVisitor {
    type Value = During;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string containing 'any', a string contaning a light value, or a sequence containing multiple light values")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut vec: Vec<Light> = Vec::new();

        while let Some(light) = seq.next_element()? {
            vec.push(light);
        }

        Ok(During::Multiple(vec))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if v == "any" {
            return Ok(During::Any);
        }

        let light = Light::deserialize(StrDeserializer::new(v))?;
        Ok(During::Single(light))
    }
}
