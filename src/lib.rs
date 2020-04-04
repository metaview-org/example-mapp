use std::sync::Mutex;
use std::time::Duration;
use std::collections::VecDeque;
use include_dir::{include_dir, DirEntry, FileSystem};
use regex::Regex;
use lazy_static::lazy_static;
use ammolite_math::*;
use wasm_bindgen::prelude::*;
use ::mlib::*;

#[derive(Default)]
pub struct SyncIO {
    out: Mutex<Vec<u8>>,
    err: Mutex<Vec<u8>>,
}

impl SyncIO {
    pub fn flush(&self) -> IO {
        IO {
            out: {
                let mut mut_out = GLOBAL_IO.out.lock().expect("Could not lock the stdout.");
                std::mem::replace(mut_out.as_mut(), Vec::new())
            },
            err: {
                let mut mut_err = GLOBAL_IO.err.lock().expect("Could not lock the stderr.");
                std::mem::replace(mut_err.as_mut(), Vec::new())
            },
        }
    }

    pub fn write_out<T: AsRef<[u8]>>(&self, bytes: impl IntoIterator<Item=T>) {
        let mut mut_out = GLOBAL_IO.out.lock().expect("Could not lock the stdout.");

        for item in bytes {
            mut_out.extend(item.as_ref());
        }
    }

    pub fn write_err<T: AsRef<[u8]>>(&self, bytes: impl IntoIterator<Item=T>) {
        let mut mut_err = GLOBAL_IO.err.lock().expect("Could not lock the stderr.");

        for item in bytes {
            mut_err.extend(item.as_ref());
        }
    }
}

lazy_static! {
    pub static ref GLOBAL_IO: SyncIO = Default::default();
}

#[allow(unused)]
macro_rules! print {
    ($($tt:tt)*) => {
        let formatted = format!($($tt)*);
        GLOBAL_IO.write_out(&[formatted.as_bytes()]);
    }
}

#[allow(unused)]
macro_rules! println {
    ($($tt:tt)*) => {
        let formatted = format!($($tt)*);
        GLOBAL_IO.write_out(&[
            formatted.as_bytes(),
            std::slice::from_ref(&('\n' as u8))
        ]);
    }
}

#[allow(unused)]
macro_rules! eprint {
    ($($tt:tt)*) => {
        let formatted = format!($($tt)*);
        GLOBAL_IO.write_err(&[formatted.as_bytes()]);
    }
}

#[allow(unused)]
macro_rules! eprintln {
    ($($tt:tt)*) => {
        let formatted = format!($($tt)*);
        GLOBAL_IO.write_err(&[
            formatted.as_bytes(),
            std::slice::from_ref(&('\n' as u8))
        ]);
    }
}

// Implementation from https://doc.rust-lang.org/std/macro.dbg.html
#[allow(unused)]
macro_rules! dbg {
    () => {
        eprintln!("[{}:{}]", file!(), line!());
    };
    ($val:expr) => {
        match $val {
            tmp => {
                eprintln!("[{}:{}] {} = {:#?}",
                    file!(), line!(), stringify!($val), &tmp);
                tmp
            }
        }
    };
    ($val:expr,) => { dbg!($val) };
    ($($val:expr),+ $(,)?) => {
        ($(dbg!($val)),+,)
    };
}

const SELECTION_DELAY: f32 = 1.0;
const ANIMATION_SPEED: f32 = 0.2;
const ENTITY_COUNT: usize = 3;

const MODEL_BUTTON_PREVIOUS_BYTES: &'static [u8] = include_bytes!("../resources/ui/button_previous.glb");
const MODEL_BUTTON_NEXT_BYTES: &'static [u8] = include_bytes!("../resources/ui/button_next.glb");
const MODEL_MARKER_BYTES: &'static [u8] = include_bytes!("../resources/ui/sphere_1m_radius.glb");

const DIR: FileSystem = include_dir!("resources/showcase");

lazy_static! {
    static ref GLOBAL_TRANSFORMATION: Mat4 = Mat4::rotation_yaw(0.0 * std::f32::consts::PI);
    static ref MODELS_MAIN_BYTES_SCALE: Vec<(&'static [u8], f32)> = {
        let files = DIR.find("**/*_(*).glb")
            .expect("Could not traverse the resource directory tree.")
            .flat_map(|dir_entry| {
                match dir_entry {
                    DirEntry::File(file) => Some(file),
                    DirEntry::Dir(_) => None,
                }
            })
            .collect::<Vec<_>>();

        if files.len() <= 0 {
            panic!("No `.glb` glTF models in the `resources/showcase` directory.")
        }

        println!("Packaged showcase models:");

        files.into_iter()
            .enumerate()
            .map(|(index, file)| {
                lazy_static! {
                    static ref PATTERN: Regex = Regex::new(r"^(?P<name>.*)_\((?P<scale>.*?)\)$").unwrap();
                }
                let stem = file.path().file_stem().unwrap().to_str().unwrap();
                let captures = PATTERN.captures(stem).unwrap();
                let scale = captures.name("scale").unwrap().as_str().parse().unwrap();

                println!("Model #{}: {:?}", index, file.path());

                (file.contents(), scale)
            })
            .collect::<Vec<_>>()
    };
}

fn construct_model_matrix(scale: f32, translation: &Vec3, rotation: &Vec3) -> Mat4 {
    Mat4::translation(translation)
        * Mat4::rotation_roll(rotation[2])
        * Mat4::rotation_yaw(rotation[1])
        * Mat4::rotation_pitch(rotation[0])
        * Mat4::scale(scale)
}

fn duration_to_seconds(duration: Duration) -> f32 {
    (duration.as_secs() as f64 + duration.subsec_nanos() as f64 / 1_000_000_000f64) as f32
}

#[derive(Debug)]
pub struct Orientation {
    direction: Vec3,
    position: Vec3,
}

pub struct RayTracingTask {
    direction: Vec3,
    total_distance: f32,
}

pub struct Selection {
    entity: Entity,
    since: Duration,
}

#[mapp]
pub struct ExampleMapp {
    elapsed: Duration,
    command_id_next: usize,
    commands: VecDeque<Command>,
    view_orientations: Option<Vec<Option<Orientation>>>,
    root_entity: Option<Entity>,
    models_main: Vec<Option<Model>>,
    model_marker: Option<Model>,
    model_button_previous: Option<Model>,
    model_button_next: Option<Model>,
    entities_main: [Option<Entity>; ENTITY_COUNT],
    entity_marker: Option<Entity>,
    entity_button_previous: Option<Entity>,
    entity_button_next: Option<Entity>,
    ray_tracing_task: Option<RayTracingTask>,
    current_main_model_index: usize,
    current_selection: Option<Selection>,
}

impl ExampleMapp {
    fn cmd(&mut self, kind: CommandKind) {
        self.commands.push_back(Command {
            id: self.command_id_next,
            kind,
        });
        self.command_id_next += 1;
    }

    fn change_main_model_index(&mut self, new_index: usize) {
        self.current_main_model_index = new_index;
        for entity in self.entities_main.clone().iter() {
            self.cmd(CommandKind::EntityModelSet {
                entity: entity.unwrap(),
                model: self.models_main[self.current_main_model_index],
            });
        }
    }

    fn change_main_model_next(&mut self) {
        let new_index = (self.current_main_model_index + 1) % MODELS_MAIN_BYTES_SCALE.len();
        self.change_main_model_index(new_index);
    }

    fn change_main_model_previous(&mut self) {
        let new_index = if self.current_main_model_index == 0 {
            MODELS_MAIN_BYTES_SCALE.len() - 1
        } else {
            self.current_main_model_index - 1
        };
        self.change_main_model_index(new_index);
    }
}

impl Mapp for ExampleMapp {
    fn new() -> Self {
        let mut result = Self {
            elapsed: Default::default(),
            command_id_next: 0,
            commands: VecDeque::new(),
            view_orientations: None,
            root_entity: None,
            models_main: vec![None; MODELS_MAIN_BYTES_SCALE.len()],
            model_marker: None,
            model_button_previous: None,
            model_button_next: None,
            entities_main: [None; ENTITY_COUNT],
            entity_marker: None,
            entity_button_previous: None,
            entity_button_next: None,
            ray_tracing_task: None,
            current_main_model_index: 0,
            current_selection: None,
        };
        result.cmd(CommandKind::EntityRootGet);
        for (model_bytes, _) in &MODELS_MAIN_BYTES_SCALE[..] {
            result.cmd(CommandKind::ModelCreate {
                data: model_bytes.into(),
            });
        }
        result.cmd(CommandKind::ModelCreate {
            data: (&MODEL_MARKER_BYTES[..]).into(),
        });
        result.cmd(CommandKind::ModelCreate {
            data: (&MODEL_BUTTON_PREVIOUS_BYTES[..]).into(),
        });
        result.cmd(CommandKind::ModelCreate {
            data: (&MODEL_BUTTON_NEXT_BYTES[..]).into(),
        });
        for _ in 0..ENTITY_COUNT {
            result.cmd(CommandKind::EntityCreate);
        }
        result.cmd(CommandKind::EntityCreate);
        result.cmd(CommandKind::EntityCreate);
        result.cmd(CommandKind::EntityCreate);
        result
    }

    fn update(&mut self, elapsed: Duration) {
        self.elapsed = elapsed;

        let secs_elapsed = duration_to_seconds(elapsed);

        for (index, entity) in self.entities_main.clone().iter().enumerate() {
            if entity.is_none() {
                return;
            }

            let transform = construct_model_matrix(
                MODELS_MAIN_BYTES_SCALE[self.current_main_model_index].1,
                &[0.0, 0.0, 2.0 + 4.0 * index as f32].into(),
                &[(secs_elapsed * ANIMATION_SPEED).sin() * 1.0, std::f32::consts::PI + (secs_elapsed * ANIMATION_SPEED).cos() * 3.0 / 2.0, 0.0].into(),
            );

            self.cmd(CommandKind::EntityTransformSet {
                entity: entity.unwrap(),
                transform: Some(&*GLOBAL_TRANSFORMATION * transform),
            });
        }
        self.cmd(CommandKind::GetViewOrientation {});
    }

    fn send_command(&mut self) -> Option<Command> {
        self.commands.pop_front()
    }

    fn receive_command_response(&mut self, response: CommandResponse) {
        // println!(self, "RECEIVED COMMAND RESPONSE: {:#?}", response);

        match response.kind {
            CommandResponseKind::EntityRootGet { root_entity } => {
                self.root_entity = Some(root_entity);
            },
            CommandResponseKind::ModelCreate { model } => {
                let model_ref = if let Some(model_main) = self.models_main.iter_mut().find(|model_main| model_main.is_none()) {
                    model_main
                } else if self.model_marker.is_none() {
                    &mut self.model_marker
                } else if self.model_button_previous.is_none() {
                    &mut self.model_button_previous
                } else if self.model_button_next.is_none() {
                    &mut self.model_button_next
                } else {
                    panic!("Too many ModelCreate commands sent.");
                };

                *model_ref = Some(model);
            },
            CommandResponseKind::EntityCreate { entity } => {
                let (model_selector, transform) = {
                    let (entity_selector, model_selector, transform) = if let Some(index) = self.entities_main.iter().position(|entity| entity.is_none()) {
                        (&mut self.entities_main[index], self.models_main[self.current_main_model_index], Mat4::scale(MODELS_MAIN_BYTES_SCALE[self.current_main_model_index].1))
                    } else if self.entity_marker.is_none() {
                        (&mut self.entity_marker, self.model_marker, Mat4::IDENTITY)
                    } else if self.entity_button_previous.is_none() {
                        (
                            &mut self.entity_button_previous,
                            self.model_button_previous,
                            construct_model_matrix(0.2, &Vec3([ 1.0, 0.0, 1.0]), &Vec3([0.0, std::f32::consts::PI, 0.0])),
                        )
                    } else if self.entity_button_next.is_none() {
                        (
                            &mut self.entity_button_next,
                            self.model_button_next,
                            construct_model_matrix(0.2, &Vec3([-1.0, 0.0, 1.0]), &Vec3([0.0, std::f32::consts::PI, 0.0])),
                        )
                    } else {
                        panic!("Too many EntityCreate commands sent.");
                    };
                    *entity_selector = Some(entity);
                    (model_selector, transform)
                };
                self.cmd(CommandKind::EntityParentSet {
                    entity,
                    parent_entity: self.root_entity,
                });
                self.cmd(CommandKind::EntityModelSet {
                    entity,
                    model: model_selector,
                });
                self.cmd(CommandKind::EntityTransformSet {
                    entity,
                    transform: Some(&*GLOBAL_TRANSFORMATION * transform),
                });
            },
            CommandResponseKind::GetViewOrientation { views_per_medium } => {
                self.view_orientations = Some(views_per_medium.into_iter()
                    .map(|views|
                        views.map(|views| {
                            let views_len = views.len();
                            let mut average_view = Mat4::ZERO;

                            for view in views {
                                average_view = average_view + view.pose;
                            }

                            average_view = average_view / (views_len as f32);
                            average_view
                        })
                        .map(|average_view| {
                            Orientation {
                                // Investigate why -z is needed instead of +z
                                direction: (&average_view * Vec4([0.0, 0.0, -1.0, 0.0])).into_projected(),
                                position:  (&average_view * Vec4([0.0, 0.0, 0.0, 1.0])).into_projected(),
                            }
                        })
                    ).collect::<Vec<_>>());

                let ray_trace_cmd = self.view_orientations.as_ref().and_then(|view_orientations| {
                    if let [Some(any)] = &view_orientations[..] {
                        Some((any.position.clone(), any.direction.clone()))
                    } else if let [Some(hmd), _] = &view_orientations[..] {
                        Some((hmd.position.clone(), hmd.direction.clone()))
                    } else {
                        None
                    }
                });

                if let Some((position, direction)) = ray_trace_cmd {
                    self.ray_tracing_task = Some(RayTracingTask {
                        direction: direction.clone(),
                        total_distance: 0.0,
                    });
                    self.cmd(CommandKind::RayTrace {
                        origin: position,
                        direction,
                    });
                }
            },
            CommandResponseKind::RayTrace { closest_intersection } => {
                // dbg!(self, &closest_intersection);

                if let Some(closest_intersection) = closest_intersection {
                    let RayTracingTask {
                        direction,
                        total_distance,
                    } = self.ray_tracing_task.take().unwrap();
                    let previous_total_distance = total_distance;
                    let total_distance = previous_total_distance + closest_intersection.distance_from_origin;

                    // Continue ray tracing from current intersection, if marker hit
                    if Some(closest_intersection.entity) == self.entity_marker {
                        self.cmd(CommandKind::RayTrace {
                            origin: closest_intersection.position + (&direction * (32.0 * std::f32::EPSILON)),
                            direction: direction.clone(),
                        });

                        self.ray_tracing_task = Some(RayTracingTask {
                            direction,
                            total_distance,
                        });
                    } else {
                        let scale = 0.02 * total_distance * self.current_selection.as_ref().map(|selection| {
                            1.0 + duration_to_seconds(self.elapsed - selection.since)
                        }).unwrap_or(1.0);
                        let transform = Mat4::translation(&closest_intersection.position)
                            * Mat4::scale(scale);

                        self.cmd(CommandKind::EntityModelSet {
                            entity: self.entity_marker.unwrap(),
                            model: self.model_marker,
                        });
                        self.cmd(CommandKind::EntityTransformSet {
                            entity: self.entity_marker.unwrap(),
                            transform: Some(&*GLOBAL_TRANSFORMATION * transform),
                        });

                        self.ray_tracing_task = None;

                        if Some(closest_intersection.entity) == self.entity_button_previous
                            || Some(closest_intersection.entity) == self.entity_button_next {
                            if self.current_selection.is_none() {
                                self.current_selection = Some(Selection {
                                    entity: closest_intersection.entity,
                                    since: self.elapsed,
                                })
                            }
                        }

                        if self.current_selection.as_ref().map(|selection| selection.entity) == Some(closest_intersection.entity) {
                            if self.elapsed - self.current_selection.as_ref().unwrap().since >= Duration::from_secs_f32(SELECTION_DELAY) {
                                if Some(closest_intersection.entity) == self.entity_button_previous {
                                    self.change_main_model_previous();
                                } else if Some(closest_intersection.entity) == self.entity_button_next {
                                    self.change_main_model_next();
                                }

                                self.current_selection = None;
                            }
                        } else {
                            self.current_selection = None;
                        }
                    }
                } else {
                    self.current_selection = None;
                    self.cmd(CommandKind::EntityModelSet {
                        entity: self.entity_marker.unwrap(),
                        model: None,
                    });
                }
            }
            _ => (),
        }
    }

    fn receive_event(&mut self, event: Event) {
        // dbg!(event);
    }

    fn flush_io(&mut self) -> IO {
        GLOBAL_IO.flush()
    }
}
