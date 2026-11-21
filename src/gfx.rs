//! The compositor. Each `El::effect` slot becomes one rune-fx layer; the mark picks the
//! effect, `effect_data` carries its few live numbers, and the slot rect is the shape.

use rune::compose::{Compositor, Frame};
use rune::render::{EffectPhase, EffectSite, KIND_EFFECT};
use rune_fx::*;

use crate::State;

pub const LAVA: u32 = 1;
pub const GLOW: u32 = 2;
pub const FIRE: u32 = 3;
pub const MESH: u32 = 4;
pub const SPARK: u32 = 5;
/// A selected checkbox: a small lava pool under the tick.
pub const CHECK: u32 = 8;
/// Row slots start here; the offset gives each row its own mark.
pub const ROW: u32 = 16;

/// Formats Rune hands effect sites besides the frame target. An isolated layer uses
/// `Rgba16Float`, and `draw_layer_in` panics unless its format is registered here.
const EFFECT_TARGETS: [wgpu::TextureFormat; 1] = [wgpu::TextureFormat::Rgba16Float];

fn renderer(device: &wgpu::Device, format: wgpu::TextureFormat) -> FxRenderer {
    let mut fx = FxRenderer::new(device, format, FxOptions { dither: true });
    for target in EFFECT_TARGETS {
        fx.add_format(device, target);
    }
    fx.set_fire_sim(device, true);
    fx
}

/// `mark` carries the effect kind in its low byte and a per-row seed above it.
/// `d` is the slot's `effect_data`. Kept to plain numbers so the view does not
/// need a shader's parameter struct.
fn layer(mark: u32, d: [f32; 4], shape: Shape, t: f32) -> Option<FxLayer> {
    let kind = mark & 0xff;
    let fx = match kind {
        LAVA => Fx::fill(MetaballParams {
            size: d[0],
            speed: d[1],
            ..MetaballParams::lava()
        }),
        GLOW => {
            if d[1] <= 0.0 {
                return None;
            }
            let color = if d[2] > 0.66 {
                rgb(0xff5a3c)
            } else if d[2] > 0.33 {
                rgb(0xffb547)
            } else {
                rgb(0x34d399)
            };
            Fx::glow(GlowParams {
                color,
                radius: d[0],
                intensity: d[1],
                ..GlowParams::default()
            })
        }
        FIRE => Fx::fire(FireParams {
            style: FireStyle::Border,
            height: d[0],
            intensity: d[1],
            embers: 8,
            ember_rise: d[2],
            scale: 16.0,
            ..FireParams::default()
        }),
        MESH => Fx::fill(MeshGradientParams {
            colors: Stops::hex(&[0x1a0710, 0x8a1f2e, 0xb85a1a, 0x4a0f2a, 0x8a3410]),
            speed: 0.2,
            ..MeshGradientParams::dusk()
        }),
        SPARK => Fx::sparkles(SparkleParams {
            color: rgb(0xffd27a),
            count: 14,
            size: 9.0,
            life: 1.1,
            intensity: 1.0,
            margin: 24.0,
            burst: Some(d[0]),
        }),
        CHECK => {
            if d[0] < 0.5 {
                return None;
            }
            Fx::fill(MetaballParams {
                colors: Stops::hex(&[0xff5e3a, 0xffa23c, 0xff3d6e]),
                base: rgb(0x3a1008),
                count: 3,
                size: 0.34,
                speed: 0.5,
                gloss: 0.6,
            })
        }
        _ if kind >= ROW => {
            if d[0] < 0.5 {
                return None;
            }
            Fx::fill(MetaballParams {
                colors: Stops::hex(&[0x5a1414, 0x8a2a12, 0x6a1030]),
                base: rgb(0x141014),
                count: 4,
                size: 0.36,
                speed: 0.3,
                gloss: 0.35,
            })
        }
        _ => return None,
    };
    let mut layer = fx.on(shape).at(t);
    if kind >= ROW {
        layer = layer.seed((mark >> 8) + 1);
    }
    Some(layer)
}

#[derive(Default)]
pub struct Gfx {
    fx: Option<FxRenderer>,
}

impl Compositor<State> for Gfx {
    fn animating(&self) -> bool {
        true
    }

    fn render(&mut self, f: &mut Frame<'_, State>) {
        let (device, queue, size) = (f.device, f.queue, f.size);
        let t = f.rt.t as f32;
        let marks = &f.rt.marks;
        let mut at: Vec<u32> = Vec::new();
        let mut layers: Vec<FxLayer> = Vec::new();
        let mut placements: Vec<FxSlot> = Vec::new();
        let mut fire = false;
        for (n, i) in f.rt.instances.iter().enumerate() {
            if i.params[1] != KIND_EFFECT {
                continue;
            }
            let mark = marks.get(n).copied().unwrap_or(0);
            let k = i.params[3];
            let [x, y, w, h] = i.rect.map(|v| v / k);
            let r = i.radii.iter().fold(0.0f32, |a, v| a.max(v.abs())) / k;
            let Some(l) = layer(mark, i.uv, Shape::rounded(x, y, w, h, r), t) else {
                continue;
            };
            fire |= (mark & 0xff) == FIRE;
            at.push(n as u32);
            layers.push(l);
            placements.push(FxSlot::default().scale(k).clip(i.clip, i.fade));
        }
        let fx = self.fx.get_or_insert_with(|| renderer(device, f.format));
        fx.set_srgb_blend(f.srgb_blend());
        fx.prepare_with_slots(
            device,
            queue,
            &FxFrame {
                viewport: [size.0, size.1],
                scale: 1.0,
                layers: &layers,
                backdrop: None,
                sources: &[],
            },
            &placements,
        );
        if fire {
            fx.simulate(queue, f.encoder, None);
        }
        let clear = f.rt.clear_color();
        f.ui.prepare(device, queue, &mut f.rt.text.atlas, &f.rt.instances, size);
        f.ui.draw_with_backdrop(
            f.encoder,
            f.target,
            clear,
            &mut |phase, s: &EffectSite<'_>| {
                let Some(k) = at.iter().position(|n| *n == s.index) else {
                    return;
                };
                if let EffectPhase::Draw(pass) = phase {
                    fx.draw_layer_in(pass, s.format, s.origin, k);
                }
            },
        );
    }
}
