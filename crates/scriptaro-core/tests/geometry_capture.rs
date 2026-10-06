use scriptaro_core::{Action, yaml};
use std::collections::BTreeMap;

#[test]
fn geometry_and_capture_compile_variables_round_trip_and_reject_invalid_input() {
    let source = "version: 2\nvariables: {app: 'Demo', output: 'shot.png'}\nsteps:\n - action: set_window_bounds\n   window: {app: {by: name, value: '${app}'}, title: Scratch}\n   bounds: {x: -100, y: 20, width: 800, height: 600}\n - action: screenshot\n   path: '${output}'\n";
    let script = yaml::compile(source, &BTreeMap::new()).unwrap().script;
    assert!(
        matches!(&script.steps[0], Action::SetWindowBounds{window,bounds,..} if window.title=="Scratch" && bounds.x == -100.0)
    );
    assert!(
        matches!(&script.steps[1], Action::Screenshot{path,region:None,timeout_ms:None} if path.to_str()==Some("shot.png"))
    );
    assert_eq!(
        script,
        yaml::from_str(&yaml::to_string(&script).unwrap()).unwrap()
    );
    for action in [
        "{action: screenshot, path: shot.jpg}",
        "{action: screenshot, path: shot.png, timeout_ms: 0}",
        "{action: screenshot, path: shot.png, overwrite: true}",
        "{action: screenshot, path: shot.png, region: {x: .nan, y: 0, width: 10, height: 10}}",
        "{action: screenshot, path: shot.png, region: {x: 0, y: 0, width: 0, height: 10}}",
        "{action: screenshot, path: shot.png, region: {x: 0, y: 0, width: 16384, height: 16384}}",
        "{action: set_window_bounds, window: {app: {by: name, value: Demo}, title: ''}, bounds: {x: 0, y: 0, width: 10, height: 10}}",
        "{action: set_window_bounds, window: {app: {by: name, value: Demo}, title: A}, bounds: {x: 0, y: 0, width: -1, height: 10}}",
    ] {
        assert!(
            yaml::from_str(&format!("version: 1\nsteps: [{action}]")).is_err(),
            "accepted {action}"
        );
    }
}
