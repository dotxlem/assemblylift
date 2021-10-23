use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct EntityManifest {
    pub entity: ManifestHeader,
    pub fields: Vec<Field>,
    pub actions: Option<Vec<Action>>,
    // pub policies: Option<Vec<Policy>>,
}

impl EntityManifest {
    pub fn read(path: &std::path::PathBuf) -> Result<Self, std::io::Error> {
        match std::fs::read_to_string(path) {
            Ok(contents) => Ok(Self::from(contents)),
            Err(why) => Err(std::io::Error::new(std::io::ErrorKind::Other, why.to_string())),
        }
    }

    pub fn verify(&self) -> Result<(), ()> {
        // TODO WARN if no caps
        // TODO ERROR if file&&from for any policy
        // TODO ERROR if cap has no args
        // TODO ERROR if invalid field type
        Ok(())
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

#[derive(Deserialize, Debug)]
pub struct ManifestHeader {
    pub name: String,
}

#[derive(Deserialize, Debug)]
pub enum Fields {
    #[serde(rename = "string")]
    String,
    #[serde(rename = "number")]
    Number,
}

#[derive(Deserialize, Debug)]
pub struct Field {
    pub name: String,
    pub r#type: Fields,
    pub attributes: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
pub enum Actions {
    #[serde(rename = "load")]
    Load,
    #[serde(rename = "store")]
    Store,
}

#[derive(Deserialize, Debug)]
pub struct Action {
    pub action: Actions,
    pub cap: Capability,
}

#[derive(Deserialize, Debug)]
pub enum Effect {
    #[serde(rename = "allow")]
    Allow,
    #[serde(rename = "deny")]
    Deny,
}

#[derive(Deserialize, Debug)]
pub struct Capability {
    pub location: String,
    pub origins: Vec<String>,
    pub effect: Effect,
}

#[derive(Deserialize, Debug)]
pub struct Policy {
    pub name: String,
    pub r#type: String,
    pub file: Option<String>,
    pub from: Option<String>,
}
