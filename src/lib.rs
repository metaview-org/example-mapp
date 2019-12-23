use std::time::Duration;
use std::io::Read;
use std::collections::VecDeque;
use ammolite_math::*;
use wasm_bindgen::prelude::*;
use lazy_static::lazy_static;
use mlib::*;

macro_rules! print {
    ($mapp:ident, $($tt:tt)*) => {
        let formatted = format!($($tt)*);
        $mapp.io.out.extend(formatted.as_bytes());
    }
}

macro_rules! println {
    ($mapp:ident, $($tt:tt)*) => {
        print!($mapp, $($tt)*);
        $mapp.io.out.push('\n' as u8)
    }
}

macro_rules! eprint {
    ($mapp:ident, $($tt:tt)*) => {
        let formatted = format!($($tt)*);
        $mapp.io.err.extend(formatted.as_bytes());
    }
}

macro_rules! eprintln {
    ($mapp:ident, $($tt:tt)*) => {
        eprint!($mapp, $($tt)*);
        $mapp.io.err.push('\n' as u8)
    }
}

// Implementation from https://doc.rust-lang.org/std/macro.dbg.html
macro_rules! dbg {
    ($mapp:ident, ) => {
        eprintln!($mapp, "[{}:{}]", file!(), line!());
    };
    ($mapp:ident, $val:expr) => {
        match $val {
            tmp => {
                eprintln!($mapp, "[{}:{}] {} = {:#?}",
                    file!(), line!(), stringify!($val), &tmp);
                tmp
            }
        }
    };
    ($mapp:ident, $val:expr,) => { dbg!($mapp, $val) };
    ($mapp:ident, $($val:expr),+ $(,)?) => {
        ($(dbg!($mapp, $val)),+,)
    };
}

// const MODEL_MAIN_BYTES: &'static [u8] = include_bytes!(env!("MODEL"));
// const MODEL_MAIN_BYTES: &'static [u8] = include_bytes!("/home/limeth/Downloads/meteor-crater-arizona/source/Meteor Crater.glb");
const MODEL_BUTTON_PREVIOUS_BYTES: &'static [u8] = include_bytes!("/home/limeth/Documents/School/mvr/metaview models/button_previous.glb");
const MODEL_BUTTON_NEXT_BYTES: &'static [u8] = include_bytes!("/home/limeth/Documents/School/mvr/metaview models/button_next.glb");
const MODELS_MAIN_LEN: usize = 4;
const MODELS_MAIN_BYTES_SCALE: [(&'static [u8], f32); MODELS_MAIN_LEN] = [
    (include_bytes!("../../ammolite/resources/DamagedHelmet/glTF-Binary/DamagedHelmet.glb"), 1.0),
    (include_bytes!("../../ammolite/resources/Corset/glTF-Binary/Corset.glb"), 40.0),
    (include_bytes!("../../ammolite/resources/AntiqueCamera/glTF-Binary/AntiqueCamera.glb"), 0.1),
    (include_bytes!("../../ammolite/resources/WaterBottle/glTF-Binary/WaterBottle.glb"), 5.0),
];
const MODEL_MARKER_BYTES: &'static [u8] = include_bytes!("../../ammolite/resources/sphere_1m_radius.glb");
const SELECTION_DELAY: f32 = 1.0;

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
    io: IO,
    state: Vec<String>,
    command_id_next: usize,
    commands: VecDeque<Command>,
    view_orientations: Option<Vec<Option<Orientation>>>,
    root_entity: Option<Entity>,
    models_main: Vec<Option<Model>>,
    model_marker: Option<Model>,
    model_button_previous: Option<Model>,
    model_button_next: Option<Model>,
    entity_main: Option<Entity>,
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
        self.cmd(CommandKind::EntityModelSet {
            entity: self.entity_main.unwrap(),
            model: self.models_main[self.current_main_model_index],
        });
    }

    fn change_main_model_next(&mut self) {
        let new_index = (self.current_main_model_index + 1) % MODELS_MAIN_LEN;
        self.change_main_model_index(new_index);
    }

    fn change_main_model_previous(&mut self) {
        let new_index = if self.current_main_model_index == 0 {
            MODELS_MAIN_LEN - 1
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
            io: Default::default(),
            state: Vec::new(),
            command_id_next: 0,
            commands: VecDeque::new(),
            view_orientations: None,
            root_entity: None,
            models_main: vec![None; MODELS_MAIN_LEN],
            model_marker: None,
            model_button_previous: None,
            model_button_next: None,
            entity_main: None,
            entity_marker: None,
            entity_button_previous: None,
            entity_button_next: None,
            ray_tracing_task: None,
            current_main_model_index: 0,
            current_selection: None,
        };
        result.cmd(CommandKind::EntityRootGet);
        for (model_bytes, _) in &MODELS_MAIN_BYTES_SCALE {
            result.cmd(CommandKind::ModelCreate {
                data: Vec::from(*model_bytes),
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
        result.cmd(CommandKind::EntityCreate);
        result.cmd(CommandKind::EntityCreate);
        result.cmd(CommandKind::EntityCreate);
        result.cmd(CommandKind::EntityCreate);
        result
    }

    fn test(&mut self, arg: String) -> Vec<String> {
        self.state.push(arg);
        self.state.clone()
    }

    fn update(&mut self, elapsed: Duration) {
        self.elapsed = elapsed;

        if self.entity_main.is_none() {
            return;
        }

        let secs_elapsed = duration_to_seconds(elapsed);
        // dbg!(self, elapsed);
        // dbg!(self, secs_elapsed);
        let anim_speed = 0.2;
        let transform = construct_model_matrix(
            MODELS_MAIN_BYTES_SCALE[self.current_main_model_index].1,
            &[0.0, 0.0, 2.0].into(),
            &[(secs_elapsed * anim_speed).sin() * 1.0, std::f32::consts::PI + (secs_elapsed * anim_speed).cos() * 3.0 / 2.0, 0.0].into(),
        );

        self.cmd(CommandKind::EntityTransformSet {
            entity: self.entity_main.unwrap(),
            transform: Some(transform),
        });
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
                    let (entity_selector, model_selector, transform) = if self.entity_main.is_none() {
                        (&mut self.entity_main, self.models_main[self.current_main_model_index], Mat4::scale(MODELS_MAIN_BYTES_SCALE[self.current_main_model_index].1))
                    } else if self.entity_marker.is_none() {
                        (&mut self.entity_marker, self.model_marker, Mat4::identity())
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
                    entity: entity,
                    parent_entity: self.root_entity,
                });
                self.cmd(CommandKind::EntityModelSet {
                    entity: entity,
                    model: model_selector,
                });
                self.cmd(CommandKind::EntityTransformSet {
                    entity: entity,
                    transform: Some(transform),
                });
            },
            CommandResponseKind::GetViewOrientation { views_per_medium } => {
                // dbg!(self, &views_per_medium);

                self.view_orientations = Some(views_per_medium.into_iter()
                    .map(|views|
                        views.map(|views| {
                            let views_len = views.len();
                            let mut average_view = Mat4::zero();

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

                // dbg!(self, &self.view_orientations);

                let ray_trace_cmd = self.view_orientations.as_ref().and_then(|view_orientations| {
                    if let [Some(hmd), _] = &view_orientations[..] {
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
                        direction: direction,
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
                            direction: direction,
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
                            transform: Some(transform),
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

    fn flush_io(&mut self) -> IO {
        std::mem::replace(&mut self.io, Default::default())
    }

    // fn get_model_matrices(&mut self, secs_elapsed: f32) -> Vec<Mat4> {
    //     fn construct_model_matrix(scale: f32, translation: &Vec3, rotation: &Vec3) -> Mat4 {
    //         Mat4::translation(translation)
    //             * Mat4::rotation_roll(rotation[2])
    //             * Mat4::rotation_yaw(rotation[1])
    //             * Mat4::rotation_pitch(rotation[0])
    //             * Mat4::scale(scale)
    //     }

    //     let matrix = construct_model_matrix(
    //         1.0,
    //         &[0.0, 0.0, 2.0].into(),
    //         &[secs_elapsed.sin() * 0.0 * 1.0, std::f32::consts::PI + secs_elapsed.cos() * 0.0 * 3.0 / 2.0, 0.0].into(),
    //     );

    //     let matrices = vec![matrix];

    //     matrices
    // }
}
