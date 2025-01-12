use bevy_gravirollback::prelude::*;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::PresentMode;

use bevy_inspector_egui::quick::WorldInspectorPlugin;

use std::time::Duration;

// This example shows a ball which is falling down and every time
// a signal is received the ball will move back up into its initial position
// and start falling from there again.
// This example will have a virtual sender that will send those signals
// at regular intervals but those signals will be artificialy delayed at random.
// The delay will cause the ball to appear to fall down further than it should.
// When the ball reaches a certain depth (which will only happen because of the delay),
// the ball will be despawned.
// But as soon as the delayed signals get received, the rollback will move back in
// time and replay those signals at their propper time. This will cause the ball
// to rewind and actually never reach the depth and therefore never despawn.

// This is a simple model of a multiplayer game where some internet trafic
// of one player (the ball) gets delayed but corrected by the rollback.
// The ball player is trying to not get low enough to despawn.
// From the perspective of the ball player he is doing everything correctly.
// Therefore there should be no way for him to trigger the despawn.
// As it would not be fair for the ball player to loose just because some
// packet got delayed it has to be corrected by the rollback, that is its job.

//the Rollback storage length (in frames)
const LEN: usize = 128;

//define our Rollback with our LEN so that we dont have to always write Rollback<T, LEN>
type Rollback<T> = bevy_gravirollback::Rollback<T, LEN>;

//copied from bevy_gravirollback::for_user and modified to use our LEN
fn make_rollback<T: Component + Default>(component: T) -> (T, Rollback<T>) {
    (component, Rollback::default())
}

fn main() {
    let mut app = App::new();

    app.add_plugins((
        DefaultPlugins,
        WorldInspectorPlugin::new(),
        RollbackPlugin::<LEN>,
        RollbackSchedulePlugin::<LEN>::default(),
        ExistencePlugin::<LEN>,
    ))

    //TODO: these should be probably automaticaly registered
    .register_type::<Rollback<Modified>>()
    .register_type::<Rollback<Frame>>()
    .register_type::<RollbackMap>()
    .register_type::<Frame>()
    .register_type::<LastFrame>()
    .register_type::<WantedFrame>()
    .register_type::<RollbackUpdateConfig>()
    //and these too
    .register_type::<RollbackID>()
    .register_type::<Exists>()
    .register_type::<Rollback<Exists>>()

    .register_type::<Rollback<Transform>>()

    .insert_resource(AmbientLight {
        color: Color::srgb(1.0,1.0,1.0),
        brightness: 0.2,
    })
    .insert_resource(ClearColor(Color::default()))

    .add_systems(Startup, setup)

    .insert_resource(UpdateTimer(Duration::from_secs(0)))
    .add_systems(Update,(
        advance_frame,
        get_input,
    ).in_set(RollbackProcessSet::HandleIO));

    RollbackSystemConfigurator::<LEN>::default()
        .add::<(
            Transform,  //even when having just one type, you have to wrap it into a tuple like this: (T,)
            //Mesh3d,   //you can register multiple at the same time
            //GlobalTransform,
            //etc...
        )>()
        .apply(&mut app);
    
    app
    .add_systems(RollbackSave,clear_resource_input_option::<PlayerInput,LEN>)
    .add_systems(RollbackUpdate, restore_resource_option::<PlayerInput,LEN>.in_set(RollbackUpdateSet::LoadInputs))
    .insert_resource(Rollback::<Option<PlayerInput>>::default())
    
    .add_systems(RollbackUpdate,(
        (
            jump,
            fall,
            ball_existence,
        ).chain()
    ).in_set(RollbackUpdateSet::Update))

    .insert_resource(WaitingInputs(Vec::new()))
    ;

    app.run();
}

#[derive(Resource, Clone, Default)]  //TODO: remove Default requirement
struct PlayerInput;

#[derive(Component)]
struct BallMarker;

fn setup(
    mut commands: Commands,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
) {
    window.single_mut().present_mode = PresentMode::AutoNoVsync;

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 30.0)
    ));
    println!("running setup");
    let id = RollbackID(0);     //I should make sure its unique
    //commands.add(spawn(spawn_ball, (Transform::from_xyz(0.0, 10.0, 0.0), id)));
    //commands.add(spawn2(|world| spawn_ball2(Transform::from_xyz(0.0, 10.0, 0.0), id, world)));
    commands.queue(spawn3(spawn_ball3(Transform::from_xyz(0.0, 10.0, 0.0), id)));
    
    /*
    let t = Transform::default();
    commands.add(spawn(move |In(id), world| {
        world.spawn((id,t)).id()
    }, RollbackID(3)));

    commands.add(spawn2(move |world| {
        world.spawn((RollbackID(4),t)).id()
    }));


    #[derive(Component)]
    struct X;
    impl X {
        fn clon(&mut self) -> X { X }
    }

    let mut x = X;
    commands.add(spawn2(move |world| {
        world.spawn((RollbackID(5),t,x.clon())).id()
    }));

    let mut x = X;
    commands.add(spawn3(move |mut commands: Commands| {
        commands.spawn((RollbackID(6),t,x.clon())).id()
    }));
    */
}

fn spawn_ball(In((transform, id)): In<(Transform, RollbackID)>, world: &mut World) -> Entity {
    spawn_ball2(transform, id, world)
}

fn spawn_ball2(transform: Transform, id: RollbackID, world: &mut World) -> Entity {
    let mut assets = world.resource_mut::<Assets<Mesh>>();
    let mesh = assets.add(Sphere::default());
    let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
    let material = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        ..Default::default()
    });

    println!("spawning ball");

    world.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material)
    )).insert((
        BallMarker,
        id,
        Rollback::<Exists>::default(),
        make_rollback(transform),  // this will contain the snapshots of Transform for this entity
    )).id()
}

fn spawn_ball3(transform: Transform, id: RollbackID) -> impl Fn(Commands, ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>) -> Entity {
    move |mut commands, mut assets, mut materials| {
        let mesh = assets.add(Sphere::default());
        let material = materials.add(StandardMaterial {
            base_color: Color::BLACK,
            ..Default::default()
        });

        println!("spawning ball");

        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material))
        ).insert((
            BallMarker,
            id,
            make_rollback(Exists(true)),
            make_rollback(transform),  // this will contain the snapshots of Transform for this entity
        )).id()
    }
}

fn fall(mut q: Query<(&mut Exists, &mut Transform), With<BallMarker>>) {
    let Ok((mut exists, mut transform)) = q.get_single_mut() else {return};
    transform.translation = transform.translation + transform.down() * 0.3;
    if transform.translation.y <= 0.0 {
        println!("despawning ball");
        exists.0 = false;
    }
}

fn ball_existence(mut q: Query<(&Exists, &mut Visibility), (With<BallMarker>, Changed<Exists>)>) {
    let Ok((exists, mut visibility)) = q.get_single_mut() else {return};
    if exists.0 {
        *visibility = Visibility::Visible;
    }else{
        *visibility = Visibility::Hidden;
    }
}

fn jump(mut q: Query<&mut Transform, With<BallMarker>>, input: Option<Res<PlayerInput>>) {
    if input.is_some() {
        let Ok(mut transform) = q.get_single_mut() else {return};
        transform.translation.y = 10.0;
    }
}

#[derive(Resource)]
struct UpdateTimer(Duration);

const FRAME_DELAY: u32 = 10;

#[derive(Resource)]
struct WaitingInputs(Vec<u64>);

fn advance_frame(
    time: Res<Time>,
    mut timer: ResMut<UpdateTimer>,
    last: Res<LastFrame>,
    current: Res<Frame>,
    mut wanted_frame: ResMut<WantedFrame>,
    mut counter: Local<u32>,
    mut waiting: ResMut<WaitingInputs>,
) {
    let delay = 1000/60;

    if last.0 == current.0 {
        if time.elapsed() - timer.0 >= Duration::from_millis(delay) {
            timer.0 += Duration::from_millis(delay);

            print!("\nadvancing frame {} ",current.0);
            wanted_frame.0 = last.0 + 1;    //this could be instead done by creating an event AdvanceFrameEvent or something like that
    
            *counter += 1;
            if *counter==FRAME_DELAY {
                *counter = 0;
    
                waiting.0.push(last.0);
            }
        }
    }
}

//TODO: this should be made by the library
#[derive(Event)]
struct NewInputEvent {
    frame: u64,
    data: (/*****/),
}

fn get_input(
    mut waiting: ResMut<WaitingInputs>,
    mut modified: ResMut<Rollback<Modified>>,
    frames: Res<Rollback<Frame>>,
    mut inputs: ResMut<Rollback<Option<PlayerInput>>>,
    //mut events: EventWriter<NewInputEvent>,
) {
    let max = 15;
    
    let count = waiting.0.len();

    let flag = count>=max || rand::random::<f32>() < -0.1 + (count as f32) / max as f32;

    if flag && count!=0 {
        let frame = waiting.0.remove(0);
        
        //this should be done automaticaly:
        let index = index::<LEN>(frame);
        if frames[index].0 == frame {
            //insert the input
            inputs.0[index] = Some(PlayerInput);

            modified[index].0 = true;
        }else{
            todo!("dropped snapshot");
        }

        //this should be used instead:
        //events.send(NewInputEvent {
        //    frame,
        //    data: (),
        //});
    }
}