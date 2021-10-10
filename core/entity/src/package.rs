use serde::Deserialize;

#[derive(Deserialize)]
pub struct EntityManifest {
    pub entity: ManifestHeader,
    pub fields: Vec<Field>,
    pub caps: Option<Vec<Capability>>,
    pub policies: Option<Vec<Policy>>,
}

impl EntityManifest {
    pub fn read(path: &std::path::PathBuf) -> Result<Self, std::io::Error> {
        match std::fs::read_to_string(path) {
            Ok(contents) => Ok(Self::from(contents)),
            Err(why) => Err(std::io::Error::new(std::io::ErrorKind::Other, why.to_string())),
        }
    }
}

impl From<String> for EntityManifest {
    fn from(string: String) -> Self {
        match toml::from_str(&string) {
            Ok(manifest) => manifest,
            Err(why) => panic!("error parsing EntityManifest: {}", why.to_string()),
        }
    }
}

#[derive(Deserialize)]
pub struct ManifestHeader {
    pub name: String,
}

#[derive(Deserialize)]
pub struct Field {
    pub name: String,
    pub r#type: String,
    pub attributes: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct Capability {
    pub action: String,
}

#[derive(Deserialize)]
pub struct Policy {
    pub name: String,
    pub r#type: String,
    pub file: Option<String>,
    pub from: Option<String>,
}
