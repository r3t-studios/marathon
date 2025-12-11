//! Minimal test to verify Transform change detection works

use std::sync::{
    Arc,
    Mutex,
};

use bevy::prelude::*;
use lib::networking::{
    NetworkedEntity,
    NetworkedTransform,
    Synced,
};
use uuid::Uuid;

#[test]
fn test_transform_change_detection_basic() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    // Add the auto_detect system
    app.add_systems(
        Update,
        lib::networking::auto_detect_transform_changes_system,
    );

    // Add a test system that runs AFTER auto_detect to check if NetworkedEntity was
    // changed We need to check DURING the frame because change detection is
    // cleared after each frame
    let was_changed = Arc::new(Mutex::new(false));
    let was_changed_clone = was_changed.clone();

    app.add_systems(
        Update,
        move |query: Query<&NetworkedEntity, Changed<NetworkedEntity>>| {
            let count = query.iter().count();
            if count > 0 {
                println!(
                    "✓ Test system detected {} changed NetworkedEntity components",
                    count
                );
                *was_changed_clone.lock().unwrap() = true;
            } else {
                println!("✗ Test system detected 0 changed NetworkedEntity components");
            }
        },
    );

    // Spawn an entity with Transform and NetworkedTransform
    let node_id = Uuid::new_v4();
    let entity_id = Uuid::new_v4();

    let _entity = app
        .world_mut()
        .spawn((
            NetworkedEntity::with_id(entity_id, node_id),
            NetworkedTransform::default(),
            Transform::from_xyz(0.0, 0.0, 0.0),
            Synced,
        ))
        .id();

    // Run one update to clear initial change detection
    println!("First update (clearing initial change detection)...");
    app.update();

    // Reset the flag
    *was_changed.lock().unwrap() = false;

    // Now modify the Transform
    {
        let mut query = app.world_mut().query::<&mut Transform>();
        for mut transform in query.iter_mut(app.world_mut()) {
            transform.translation.x = 10.0;
        }
    }

    println!("Modified Transform, running second update...");

    // Run update - should trigger auto_detect_transform_changes_system
    app.update();

    // Check if our test system detected the change
    let result = *was_changed.lock().unwrap();
    println!("Was NetworkedEntity marked as changed? {}", result);
    assert!(
        result,
        "NetworkedEntity should be marked as changed after Transform modification"
    );
}
