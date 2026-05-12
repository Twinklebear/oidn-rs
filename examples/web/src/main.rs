use web_sys::{HtmlElement, KeyboardEvent, PointerEvent};
use yew::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SceneKey {
    OakGrove,
    CornellBox,
    Teapot,
    VeachAjar,
    ExrStillLife,
    ExrMtTamWest,
    FighterJet,
}

struct Scene {
    key: SceneKey,
    title: &'static str,
    noisy: &'static str,
    denoised: &'static str,
    noisy_alt: &'static str,
    denoised_alt: &'static str,
}

const SCENES: &[Scene] = &[
    Scene {
        key: SceneKey::OakGrove,
        title: "Oak Grove",
        noisy: "./assets/oak-grove-noisy.webp",
        denoised: "./assets/oak-grove-denoised.webp",
        noisy_alt: "Noisy oak grove image",
        denoised_alt: "Denoised oak grove image",
    },
    Scene {
        key: SceneKey::CornellBox,
        title: "Cornell Box",
        noisy: "./assets/cornell-box-noisy.webp",
        denoised: "./assets/cornell-box-denoised.webp",
        noisy_alt: "Noisy Cornell box image",
        denoised_alt: "Denoised Cornell box image",
    },
    Scene {
        key: SceneKey::Teapot,
        title: "Teapot",
        noisy: "./assets/teapot-noisy.webp",
        denoised: "./assets/teapot-denoised.webp",
        noisy_alt: "Noisy teapot image",
        denoised_alt: "Denoised teapot image",
    },
    Scene {
        key: SceneKey::VeachAjar,
        title: "Veach Ajar",
        noisy: "./assets/veach-ajar-noisy.webp",
        denoised: "./assets/veach-ajar-denoised.webp",
        noisy_alt: "Noisy Veach Ajar image",
        denoised_alt: "Denoised Veach Ajar image",
    },
    Scene {
        key: SceneKey::ExrStillLife,
        title: "EXR Still Life",
        noisy: "./assets/exr-still-life-noisy.webp",
        denoised: "./assets/exr-still-life-denoised.webp",
        noisy_alt: "Noisy EXR still life image",
        denoised_alt: "Denoised EXR still life image",
    },
    Scene {
        key: SceneKey::ExrMtTamWest,
        title: "EXR Mt Tam West",
        noisy: "./assets/exr-mt-tam-west-noisy.webp",
        denoised: "./assets/exr-mt-tam-west-denoised.webp",
        noisy_alt: "Noisy EXR Mount Tamalpais west view image",
        denoised_alt: "Denoised EXR Mount Tamalpais west view image",
    },
    Scene {
        key: SceneKey::FighterJet,
        title: "Fighter Jet",
        noisy: "./assets/fighter-jet-noisy.webp",
        denoised: "./assets/fighter-jet-denoised.webp",
        noisy_alt: "Noisy fighter jet image",
        denoised_alt: "Denoised fighter jet image",
    },
];

impl SceneKey {
    fn scene(self) -> &'static Scene {
        SCENES
            .iter()
            .find(|scene| scene.key == self)
            .expect("scene key has a scene")
    }
}

#[function_component(App)]
fn app() -> Html {
    let active_scene = use_state(|| SceneKey::OakGrove);
    let split = use_state(|| 50.0_f64);
    let dragging = use_state(|| false);
    let comparison = use_node_ref();

    let scene = active_scene.scene();
    let set_split = {
        let split = split.clone();
        Callback::from(move |next_split: f64| {
            split.set(next_split.clamp(0.0, 100.0));
        })
    };

    let split_from_pointer = {
        let comparison = comparison.clone();
        let set_split = set_split.clone();
        Callback::from(move |event: PointerEvent| {
            let Some(comparison) = comparison.cast::<HtmlElement>() else {
                return;
            };
            let rect = comparison.get_bounding_client_rect();
            let width = rect.width();
            if width <= 0.0 {
                return;
            }

            let next_split = ((event.client_x() as f64 - rect.left()) / width) * 100.0;
            set_split.emit(next_split);
        })
    };

    let on_pointer_down = {
        let comparison = comparison.clone();
        let dragging = dragging.clone();
        let split_from_pointer = split_from_pointer.clone();
        Callback::from(move |event: PointerEvent| {
            if event.pointer_type() == "mouse" && event.button() != 0 {
                return;
            }

            event.prevent_default();
            if let Some(comparison) = comparison.cast::<HtmlElement>() {
                let _ = comparison.set_pointer_capture(event.pointer_id());
            }
            dragging.set(true);
            split_from_pointer.emit(event);
        })
    };

    let on_pointer_move = {
        let dragging = dragging.clone();
        let split_from_pointer = split_from_pointer.clone();
        Callback::from(move |event: PointerEvent| {
            if *dragging {
                event.prevent_default();
                split_from_pointer.emit(event);
            }
        })
    };

    let stop_drag = {
        let comparison = comparison.clone();
        let dragging = dragging.clone();
        Callback::from(move |event: PointerEvent| {
            if let Some(comparison) = comparison.cast::<HtmlElement>() {
                let _ = comparison.release_pointer_capture(event.pointer_id());
            }
            dragging.set(false);
        })
    };

    let on_key_down = {
        let split = split.clone();
        let set_split = set_split.clone();
        Callback::from(move |event: KeyboardEvent| {
            let step = if event.shift_key() { 10.0 } else { 2.0 };
            match event.key().as_str() {
                "ArrowLeft" => {
                    event.prevent_default();
                    set_split.emit(*split - step);
                }
                "ArrowRight" => {
                    event.prevent_default();
                    set_split.emit(*split + step);
                }
                "Home" => {
                    event.prevent_default();
                    set_split.emit(0.0);
                }
                "End" => {
                    event.prevent_default();
                    set_split.emit(100.0);
                }
                _ => {}
            }
        })
    };

    html! {
        <main class="shell">
            <header>
                <div>
                    <p class="eyebrow">{"oidn-rs web example"}</p>
                    <h1>{"Denoise Examples"}</h1>
                </div>
                <a class="docs-link" href="./docs/oidn/index.html">{"Rust docs"}</a>
            </header>

            <div class="workspace">
                <section class="viewer" aria-label="Noisy and denoised comparison">
                    <div class="viewer-head">
                        <h2>{scene.title}</h2>
                    </div>

                    <div
                        ref={comparison}
                        class="comparison"
                        style={format!("--split: {:.2}%;", *split)}
                        onpointerdown={on_pointer_down}
                        onpointermove={on_pointer_move}
                        onpointerup={stop_drag.clone()}
                        onpointercancel={stop_drag}
                    >
                        <img src={scene.noisy} alt={scene.noisy_alt} />
                        <img class="denoised" src={scene.denoised} alt={scene.denoised_alt} />
                        <button
                            class="divider"
                            type="button"
                            aria-label="Drag to compare noisy and denoised images"
                            aria-valuemin="0"
                            aria-valuemax="100"
                            aria-valuenow={format!("{:.0}", *split)}
                            onkeydown={on_key_down}
                        />
                    </div>
                </section>

                <aside class="library" aria-label="Example scenes">
                    {SCENES.iter().map(|scene| {
                        let key = scene.key;
                        let active = key == *active_scene;
                        let active_scene = active_scene.clone();
                        html! {
                            <button
                                class={classes!("scene-button", active.then_some("active"))}
                                type="button"
                                aria-pressed={active.to_string()}
                                onclick={Callback::from(move |_| active_scene.set(key))}
                            >
                                <img src={scene.denoised} alt="" />
                                <span>{scene.title}</span>
                            </button>
                        }
                    }).collect::<Html>()}
                </aside>
            </div>
        </main>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
