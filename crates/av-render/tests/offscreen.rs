//! Blueprint A9 gate: draw scenes into an offscreen RGBA8 target on the real GPU and read them
//! back. Skips (with a note) when the machine has no adapter, as CI runners may not.

use av_protocol::scene::{Scene, builtin};
use av_render::{Composer, Gpu, Mesh, View, instance};

fn gpu() -> Option<Gpu> {
    match Gpu::new(instance(), None) {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("skipped: {e}");
            None
        }
    }
}

fn render(gpu: &mut Gpu, scene: &Scene, w: u32, h: u32) -> Vec<u8> {
    let bars: Vec<f32> = (0..1024)
        .map(|i| (i as f32 * 0.02).sin() * 0.5 + 0.5)
        .collect();
    let wave: Vec<f32> = (0..800).map(|i| (i as f32 * 0.07).sin() * 0.6).collect();
    let mut c = Composer::default();
    let mut m = Mesh::default();
    c.compose(
        scene,
        &bars,
        Some(&wave),
        View {
            width: w as f32,
            height: h as f32,
            scale: 1.0,
            display_count: None,
        },
        &mut m,
    );
    gpu.render_offscreen(w, h, &m, [0.0; 4])
}

#[test]
fn premultiplied_invariant_holds_and_the_background_stays_clear() {
    let Some(mut gpu) = gpu() else { return };
    eprintln!("adapter: {}", gpu.adapter_name());
    for id in ["bars", "concentric"] {
        let mut scene = builtin(id);
        scene.opacity = 0.7;
        scene.scope.enabled = true;
        let px = render(&mut gpu, &scene, 320, 200);
        assert_eq!(px.len(), 320 * 200 * 4);
        let mut drawn = 0;
        for p in px.as_chunks::<4>().0 {
            assert!(
                p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3],
                "{id}: {p:?} not premultiplied"
            );
            drawn += (p[3] > 0) as usize;
        }
        assert!(drawn > 500, "{id}: only {drawn} pixels drawn");
        // The corners stay fully transparent (the clear colour shows through).
        assert_eq!(&px[0..4], &[0, 0, 0, 0], "{id}: top-left corner");
    }
}

#[test]
fn rendering_is_deterministic() {
    let Some(mut gpu) = gpu() else { return };
    let scene = builtin("concentric");
    let a = render(&mut gpu, &scene, 256, 256);
    let b = render(&mut gpu, &scene, 256, 256);
    assert_eq!(a, b);
}
