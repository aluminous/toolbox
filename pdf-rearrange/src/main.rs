mod pdf;

use js_sys::Uint8Array;
use leptos::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::{DragEvent, Event, FileReader, HtmlInputElement, MouseEvent};

// ── JS bindings ───────────────────────────────────────────────────────────────

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = renderPdfPage)]
    fn render_pdf_page(bytes: Uint8Array, canvas_id: &str, page_num: u32);
}

// ── Data types ────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
struct PdfPage {
    /// Stable unique id (uuid) — used as React-style key and for selection tracking.
    id: String,
    filename: String,
    pdf_bytes: Arc<Vec<u8>>,
    page_num: u32,
}

// ── File loading ──────────────────────────────────────────────────────────────

fn load_file(file: web_sys::File, input_pages: RwSignal<Vec<PdfPage>>) {
    let filename = file.name();
    let reader = FileReader::new().unwrap();
    let reader2 = reader.clone();

    let cb = Closure::<dyn FnMut()>::new(move || {
        let Ok(result) = reader2.result() else { return };
        let bytes = Arc::new(Uint8Array::new(&result).to_vec());
        let count = match lopdf::Document::load_mem(&bytes) {
            Ok(doc) => doc.get_pages().len() as u32,
            Err(e) => {
                web_sys::console::error_1(&format!("PDF parse error: {e}").into());
                return;
            }
        };
        let fname = filename.clone();
        let pages: Vec<PdfPage> = (1..=count)
            .map(|n| PdfPage {
                id: uuid::Uuid::new_v4().to_string(),
                filename: fname.clone(),
                pdf_bytes: Arc::clone(&bytes),
                page_num: n,
            })
            .collect();
        input_pages.update(|v| v.extend(pages));
    });

    reader.set_onloadend(Some(cb.as_ref().unchecked_ref()));
    reader.read_as_array_buffer(&file).unwrap();
    cb.forget();
}

fn load_files(files: web_sys::FileList, input_pages: RwSignal<Vec<PdfPage>>) {
    for i in 0..files.length() {
        if let Some(f) = files.get(i) {
            load_file(f, input_pages);
        }
    }
}

// ── Download helper ───────────────────────────────────────────────────────────

fn trigger_download(bytes: Vec<u8>) {
    use wasm_bindgen::JsCast;
    let arr = Uint8Array::from(bytes.as_slice());
    let seq = js_sys::Array::of1(&arr);
    let bag = web_sys::BlobPropertyBag::new();
    bag.set_type("application/pdf");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&seq, &bag)
    .unwrap();
    let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap();
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let a = document.create_element("a").unwrap();
    a.set_attribute("href", &url).unwrap();
    a.set_attribute("download", "output.pdf").unwrap();
    document.body().unwrap().append_child(&a).unwrap();
    a.unchecked_ref::<web_sys::HtmlElement>().click();
    document.body().unwrap().remove_child(&a).unwrap();
    web_sys::Url::revoke_object_url(&url).unwrap();
}

// ── PageCard component ────────────────────────────────────────────────────────

/// Renders a single page thumbnail. Works for both input and output panels.
#[component]
fn PageCard(
    page: PdfPage,
    /// Whether this card shows a "selected" highlight
    selected: Signal<bool>,
    /// Called when the card is clicked (receives MouseEvent for modifier detection)
    on_card_click: Callback<MouseEvent>,
    /// If Some, show a remove button calling this callback
    #[prop(optional)]
    on_remove: Option<Callback<()>>,
    /// Drag-data prefix: "in" for input panel, "out" for output panel
    drag_prefix: &'static str,
    /// If Some, update this signal with this card's id when dragged over (for drop-before)
    #[prop(optional)]
    drop_signal: Option<RwSignal<Option<String>>>,
    /// If Some, called on dragstart to produce the drag data string (overrides default).
    /// Use this to implement multi-select drag.
    #[prop(optional)]
    drag_data_fn: Option<Callback<(), String>>,
) -> impl IntoView {
    let canvas_id = format!("canvas-{}", page.id);
    let canvas_id_drag = canvas_id.clone(); // captured by dragstart closure
    let bytes = Arc::clone(&page.pdf_bytes);
    let page_num = page.page_num;
    let label = format!("{} p.{}", page.filename, page.page_num);
    let drag_id = format!("{}:{}", drag_prefix, page.id);
    let hover_id = page.id.clone();

    // Render thumbnail after canvas mounts
    let cid = canvas_id.clone();
    Effect::new(move |_| {
        let bytes = Arc::clone(&bytes);
        let id = cid.clone();
        spawn_local(async move {
            render_pdf_page(Uint8Array::from(bytes.as_slice()), &id, page_num);
        });
    });

    view! {
        <div
            class="page-card"
            class:selected=selected
            draggable="true"
            on:click=move |e: MouseEvent| on_card_click.run(e)
            on:dragstart=move |e: DragEvent| {
                if let Some(dt) = e.data_transfer() {
                    let data = if let Some(f) = drag_data_fn {
                        f.run(())
                    } else {
                        drag_id.clone()
                    };
                    let _ = dt.set_data("text/plain", &data);
                    // Use the rendered thumbnail as the drag ghost image
                    if let Some(el) = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.get_element_by_id(&canvas_id_drag))
                    {
                        use wasm_bindgen::JsCast;
                        let canvas = el.unchecked_ref::<web_sys::HtmlCanvasElement>();
                        let _ = dt.set_drag_image(
                            canvas,
                            (canvas.width() / 2) as i32,
                            (canvas.height() / 2) as i32,
                        );
                    }
                }
            }
            on:dragover=move |e: DragEvent| {
                e.prevent_default();
                if let Some(sig) = drop_signal {
                    sig.set(Some(hover_id.clone()));
                }
            }
        >
            <canvas id=canvas_id />
            <span class="page-label">{label}</span>
            {on_remove.map(|rm| view! {
                <button class="remove-btn"
                    on:click=move |e: MouseEvent| { e.stop_propagation(); rm.run(()); }
                >"×"</button>
            })}
        </div>
    }
}

// ── App ───────────────────────────────────────────────────────────────────────

#[component]
fn App() -> impl IntoView {
    let input_pages = RwSignal::new(Vec::<PdfPage>::new());
    let output_pages = RwSignal::new(Vec::<PdfPage>::new());
    let selected_ids = RwSignal::new(HashSet::<String>::new());
    let last_selected = RwSignal::new(Option::<String>::None);
    let drag_over_output = RwSignal::new(false);
    let drop_before_id = RwSignal::new(Option::<String>::None);

    // ── File input ────────────────────────────────────────────────────────────

    let on_file_input = move |e: Event| {
        let input = event_target::<HtmlInputElement>(&e);
        if let Some(files) = input.files() {
            load_files(files, input_pages);
        }
        // reset so the same file can be re-selected
        input.set_value("");
    };

    // File drag-drop anywhere on the page
    let on_app_dragover = move |e: DragEvent| {
        if is_file_drag(&e) {
            e.prevent_default();
        }
    };
    let on_app_drop = move |e: DragEvent| {
        if is_file_drag(&e) {
            e.prevent_default();
            if let Some(files) = e.data_transfer().and_then(|dt| dt.files()) {
                load_files(files, input_pages);
            }
        }
    };

    // ── Toolbar actions ───────────────────────────────────────────────────────

    let on_select_all = move |_| {
        let ids: HashSet<String> = input_pages
            .get_untracked()
            .iter()
            .map(|p| p.id.clone())
            .collect();
        selected_ids.set(ids);
    };

    let on_add_selected = move |_| {
        let ids = selected_ids.get_untracked();
        let to_add: Vec<PdfPage> = input_pages
            .get_untracked()
            .into_iter()
            .filter(|p| ids.contains(&p.id))
            .map(|p| PdfPage { id: uuid::Uuid::new_v4().to_string(), ..p })
            .collect();
        if !to_add.is_empty() {
            output_pages.update(|v| v.extend(to_add));
            selected_ids.update(|s| s.clear());
        }
    };

    let on_clear = move |_| {
        output_pages.set(vec![]);
    };

    let on_save = move |_| {
        let pages: Vec<(Arc<Vec<u8>>, u32)> = output_pages
            .get_untracked()
            .iter()
            .map(|p| (Arc::clone(&p.pdf_bytes), p.page_num))
            .collect();
        if pages.is_empty() {
            return;
        }
        spawn_local(async move {
            match pdf::build_output_pdf(&pages) {
                Ok(bytes) => trigger_download(bytes),
                Err(e) => {
                    web_sys::console::error_1(&format!("PDF build error: {e}").into());
                }
            }
        });
    };

    // ── Output panel drop ─────────────────────────────────────────────────────

    let on_output_dragover = move |e: DragEvent| {
        e.prevent_default();
        drag_over_output.set(true);
    };
    let on_output_dragleave = move |_: DragEvent| {
        drag_over_output.set(false);
    };
    let on_output_drop = move |e: DragEvent| {
        e.prevent_default();
        drag_over_output.set(false);
        let data = e
            .data_transfer()
            .and_then(|dt| dt.get_data("text/plain").ok())
            .unwrap_or_default();
        let before = drop_before_id.get_untracked();
        drop_before_id.set(None);
        handle_drop(&data, before, input_pages, output_pages);
    };

    view! {
        <div class="app" on:dragover=on_app_dragover on:drop=on_app_drop>

            // ── Toolbar ───────────────────────────────────────────────────────
            <header class="toolbar">
                <h1>"PDF Rearrange"</h1>
                <label class="btn">
                    "Add PDFs"
                    <input type="file" multiple=true accept=".pdf"
                        style="display:none" on:change=on_file_input />
                </label>
                <button class="btn" on:click=on_select_all>"Select All"</button>
                <button class="btn" on:click=on_add_selected>
                    "Add Selected →"
                </button>
                <button class="btn" on:click=on_clear>"Clear Output"</button>
                <button class="btn btn-primary" on:click=on_save>"Save PDF"</button>
            </header>

            <main class="panels">

                // ── Input panel ───────────────────────────────────────────────
                <section class="panel">
                    <h2>"Source Pages"</h2>
                    <div class="hint">
                        "Drop PDFs anywhere · click to select · shift+click to range select"
                    </div>
                    <div class="page-grid">
                        <For
                            each=move || input_pages.get()
                            key=|p| p.id.clone()
                            children=move |page| {
                                let pid = StoredValue::new(page.id.clone());
                                let is_selected = Signal::derive(move ||
                                    selected_ids.get().contains(pid.get_value().as_str())
                                );
                                let drag_fn = Callback::new(move |_: ()| {
                                    let ids = selected_ids.get_untracked();
                                    let my_id = pid.get_value();
                                    if ids.contains(&my_id) && ids.len() > 1 {
                                        let ordered = input_pages
                                            .get_untracked()
                                            .iter()
                                            .filter(|p| ids.contains(&p.id))
                                            .map(|p| p.id.clone())
                                            .collect::<Vec<_>>()
                                            .join("|");
                                        format!("in-multi:{}", ordered)
                                    } else {
                                        format!("in:{}", my_id)
                                    }
                                });
                                view! {
                                    <PageCard
                                        page=page
                                        selected=is_selected
                                        on_card_click=Callback::new(move |e: MouseEvent| {
                                            let id = pid.get_value();
                                            if e.shift_key() {
                                                range_select(id, input_pages, selected_ids, last_selected);
                                            } else {
                                                selected_ids.update(|s| {
                                                    if s.contains(&id) { s.remove(&id); }
                                                    else { s.insert(id.clone()); }
                                                });
                                                last_selected.set(Some(id));
                                            }
                                        })
                                        drag_prefix="in"
                                        drag_data_fn=drag_fn
                                    />
                                }
                            }
                        />
                        <Show when=move || input_pages.get().is_empty()>
                            <p class="empty-msg">"Drop PDF files here to get started"</p>
                        </Show>
                    </div>
                </section>

                // ── Output panel ──────────────────────────────────────────────
                <section
                    class="panel output-panel"
                    class:drag-over=drag_over_output
                    on:dragover=on_output_dragover
                    on:dragleave=on_output_dragleave
                    on:drop=on_output_drop
                >
                    <h2>"Output"</h2>
                    <div class="hint">"Drag pages here · drag within to reorder · ✕ to remove"</div>
                    <div class="page-grid">
                        <For
                            each=move || output_pages.get()
                            key=|p| p.id.clone()
                            children=move |page| {
                                let pid = StoredValue::new(page.id.clone());
                                view! {
                                    <PageCard
                                        page=page
                                        selected=Signal::derive(|| false)
                                        on_card_click=Callback::new(|_| {})
                                        on_remove=Callback::new(move |_| {
                                            let id = pid.get_value();
                                            output_pages.update(|v| v.retain(|p| p.id != id));
                                        })
                                        drag_prefix="out"
                                        drop_signal=drop_before_id
                                    />
                                }
                            }
                        />
                        <Show when=move || output_pages.get().is_empty()>
                            <p class="empty-msg">"Drag pages here or use Add Selected →"</p>
                        </Show>
                    </div>
                </section>

            </main>
        </div>
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn is_file_drag(e: &DragEvent) -> bool {
    e.data_transfer()
        .map(|dt| {
            let types = dt.types();
            (0..types.length())
                .any(|i| types.get(i).as_string().as_deref() == Some("Files"))
        })
        .unwrap_or(false)
}

fn range_select(
    id: String,
    input_pages: RwSignal<Vec<PdfPage>>,
    selected_ids: RwSignal<HashSet<String>>,
    last_selected: RwSignal<Option<String>>,
) {
    let pages = input_pages.get_untracked();
    let curr = pages.iter().position(|p| p.id == id);
    let last = last_selected
        .get_untracked()
        .and_then(|lid| pages.iter().position(|p| p.id == lid));
    if let (Some(ci), Some(li)) = (curr, last) {
        let (lo, hi) = if ci < li { (ci, li) } else { (li, ci) };
        selected_ids.update(|s| {
            for p in &pages[lo..=hi] {
                s.insert(p.id.clone());
            }
        });
    } else {
        selected_ids.update(|s| {
            s.insert(id.clone());
        });
    }
    last_selected.set(Some(id));
}

/// Process a drop event: parse drag data, find the page, insert it into output.
fn handle_drop(
    data: &str,
    before_id: Option<String>,
    input_pages: RwSignal<Vec<PdfPage>>,
    output_pages: RwSignal<Vec<PdfPage>>,
) {
    if let Some(ids_str) = data.strip_prefix("in-multi:") {
        let ids: Vec<&str> = ids_str.split('|').collect();
        let pages_to_add: Vec<PdfPage> = input_pages
            .get_untracked()
            .into_iter()
            .filter(|p| ids.contains(&p.id.as_str()))
            .map(|p| PdfPage { id: uuid::Uuid::new_v4().to_string(), ..p })
            .collect();
        output_pages.update(|v| {
            if let Some(bid) = &before_id {
                if let Some(idx) = v.iter().position(|p| &p.id == bid) {
                    for (i, p) in pages_to_add.into_iter().enumerate() {
                        v.insert(idx + i, p);
                    }
                    return;
                }
            }
            v.extend(pages_to_add);
        });
        return;
    }

    let page = if let Some(src_id) = data.strip_prefix("in:") {
        // From input panel — clone the page with a fresh id so the same
        // source page can appear multiple times in output
        input_pages
            .get_untracked()
            .iter()
            .find(|p| p.id == src_id)
            .map(|p| PdfPage { id: uuid::Uuid::new_v4().to_string(), ..p.clone() })
    } else if let Some(src_id) = data.strip_prefix("out:") {
        // Reordering within output — remove from current position
        let mut pages = output_pages.get_untracked();
        if let Some(idx) = pages.iter().position(|p| p.id == src_id) {
            let p = pages.remove(idx);
            output_pages.set(pages);
            Some(p)
        } else {
            None
        }
    } else {
        None
    };

    if let Some(page) = page {
        output_pages.update(|v| {
            if let Some(bid) = &before_id {
                if let Some(idx) = v.iter().position(|p| &p.id == bid) {
                    v.insert(idx, page);
                    return;
                }
            }
            v.push(page);
        });
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
