use std::path::PathBuf;
use assemblylift_core_entity::package;

fn main() {
    let user_entity = package::EntityManifest::read(&PathBuf::from(r"./examples/user.toml"))
        .expect("could not read user.toml");
    println!("Deserialized entity: \n{:?}\n", user_entity);

    let is_valid = user_entity.verify().is_ok();
    println!("Valid: {}", is_valid);
}
