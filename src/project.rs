use anyhow::{anyhow, bail, Context};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use std::path::Path;
use std::{fs::File, path::PathBuf};

use crate::action::ActionRegistrationExt;
use crate::exporter::phichain::PhiChainExporter;
use crate::exporter::Exporter;
use crate::{
    audio::SpawnAudioEvent,
    loader::{phichain::PhiChainLoader, Loader},
    notification::{ToastsExt, ToastsStorage},
    serialization::PhiChainChart,
    tab::game::illustration::SpawnIllustrationEvent,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectMeta {
    pub composer: String,
    pub charter: String,
    pub illustrator: String,
    pub name: String,
    pub level: String,
}

#[derive(Resource)]
pub struct Project {
    pub path: ProjectPath,
    pub meta: ProjectMeta,
}

impl Project {
    pub fn load(root_dir: PathBuf) -> anyhow::Result<Self> {
        ProjectPath(root_dir).into_project()
    }
}

pub struct ProjectPath(PathBuf);

impl ProjectPath {
    pub fn chart_path(&self) -> PathBuf {
        self.0.join("chart.json")
    }

    pub fn sub_path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.0.join(path)
    }

    fn find_file(&self, name: &str, allowed_extensions: &[impl ToString]) -> Option<PathBuf> {
        std::fs::read_dir(&self.0)
            .ok()?
            .filter_map(Result::ok)
            .map(|x| x.path())
            .find(|path| {
                path.is_file()
                    && path.file_stem() == Some(name.as_ref())
                    && path.extension().map_or(false, |ext| {
                        allowed_extensions
                            .iter()
                            .any(|allowed| *allowed.to_string() == *ext)
                    })
            })
    }

    pub fn music_path(&self) -> Option<PathBuf> {
        self.find_file("music", &["wav", "mp3", "ogg", "flac"])
    }

    pub fn illustration_path(&self) -> Option<PathBuf> {
        self.find_file("illustration", &["png", "jpg", "jpeg"])
    }

    pub fn meta_path(&self) -> PathBuf {
        self.0.join("meta.json")
    }

    pub fn into_project(self) -> anyhow::Result<Project> {
        if !self.chart_path().is_file() {
            bail!("chart.json is missing");
        }
        if !self
            .music_path()
            .ok_or(anyhow!("Could not find music file in project"))?
            .is_file()
        {
            bail!("music.wav is missing");
        }
        if !self
            .illustration_path()
            .ok_or(anyhow!("Could not find illustration file in project"))?
            .is_file()
        {
            bail!("illustration.png is missing");
        }
        if !self.meta_path().is_file() {
            bail!("meta.json is missing");
        }

        let meta_file = File::open(self.meta_path()).context("Failed to open meta file")?;
        let meta: ProjectMeta = serde_json::from_reader(meta_file).context("Invalid meta file")?;

        let chart_file = File::open(self.chart_path()).context("Failed to open chart file")?;
        // just do validation here
        let _: PhiChainChart = serde_json::from_reader(chart_file).context("Invalid chart")?;

        Ok(Project { path: self, meta })
    }
}

/// A [Condition] represents the project is loaded
pub fn project_loaded() -> impl Condition<()> {
    resource_exists::<Project>.and_then(|| true)
}

/// A [Condition] represents the project is not loaded
pub fn project_not_loaded() -> impl Condition<()> {
    resource_exists::<Project>.map(|x| !x)
}

pub struct ProjectPlugin;

impl Plugin for ProjectPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<LoadProjectEvent>()
            .add_systems(Update, load_project_system)
            .register_action("phichain.project.save", save_project_system);
    }
}

fn save_project_system(world: &mut World) {
    if let Ok(chart) = PhiChainExporter::export(world) {
        let project = world.resource::<Project>();
        let chart_result = std::fs::write(project.path.chart_path(), chart);
        let meta_result = std::fs::write(project.path.meta_path(), serde_json::to_string(&project.meta).unwrap());
        
        let mut toasts = world.resource_mut::<ToastsStorage>();
        match chart_result.and(meta_result) {
            Ok(_) => {
                toasts.success(t!("project.save.succeed"));
            }
            Err(error) => {
                toasts.error(t!("project.save.failed", error = error));
            }
        }
    }
}

#[derive(Event, Debug)]
pub struct LoadProjectEvent(pub PathBuf);

fn load_project_system(
    mut commands: Commands,
    mut events: EventReader<LoadProjectEvent>,
    mut toasts: ResMut<ToastsStorage>,
) {
    if events.len() > 1 {
        warn!("Multiple projects are requested, ignoring previous ones");
    }

    if let Some(event) = events.read().last() {
        match Project::load(event.0.clone()) {
            Ok(project) => {
                // unwrap: if Project::load is ok, illustration_path() must return Some
                let illustration_path = project.path.illustration_path().unwrap();
                // TODO: maybe make this load_illustration(PathBuf, mut Commands) for better error handling
                commands.add(|world: &mut World| {
                    world.send_event(SpawnIllustrationEvent(illustration_path));
                });

                // unwrap: if Project::load is ok, illustration_path() must return Some
                let audio_path = project.path.music_path().unwrap();
                // TODO: maybe make this load_music(PathBuf, mut Commands) for better error handling
                commands.add(|world: &mut World| {
                    world.send_event(SpawnAudioEvent(audio_path));
                });

                let file = File::open(project.path.chart_path()).unwrap();
                PhiChainLoader::load(file, &mut commands);
                commands.insert_resource(project);
            }
            Err(error) => {
                toasts.error(format!("Failed to open project: {:?}", error));
            }
        }
    }

    events.clear();
}

pub fn create_project(
    root_path: PathBuf,
    music_path: PathBuf,
    illustration_path: PathBuf,
    project_meta: ProjectMeta,
) -> anyhow::Result<()> {
    let project_path = ProjectPath(root_path);

    let mut target_music_path = project_path.sub_path("music");
    if let Some(ext) = music_path.extension() {
        target_music_path.set_extension(ext);
    }

    std::fs::copy(music_path, target_music_path).context("Failed to copy music file")?;

    let mut target_illustration_path = project_path.sub_path("illustration");
    if let Some(ext) = illustration_path.extension() {
        target_illustration_path.set_extension(ext);
    }

    std::fs::copy(illustration_path, target_illustration_path)
        .context("Failed to copy illustration file")?;

    let meta_string = serde_json::to_string_pretty(&project_meta).unwrap();
    std::fs::write(project_path.meta_path(), meta_string).context("Failed to write meta")?;

    let chart_string = serde_json::to_string_pretty(&PhiChainChart::default()).unwrap();
    std::fs::write(project_path.chart_path(), chart_string).context("Failed to write chart")?;

    Ok(())
}
