use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::HashMap;
use std::sync::Arc;

/// Build an output PDF from a list of (source_pdf_bytes, 1-based page_number) pairs.
/// Pages from the same source file share an Arc, so we load each unique PDF only once.
pub fn build_output_pdf(requests: &[(Arc<Vec<u8>>, u32)]) -> Result<Vec<u8>, String> {
    if requests.is_empty() {
        return Err("No pages requested".into());
    }

    // --- 1. Deduplicate source PDFs by Arc pointer ---
    let mut groups: Vec<(Arc<Vec<u8>>, Vec<(usize, u32)>)> = vec![];
    for (i, (bytes, page_num)) in requests.iter().enumerate() {
        let pos = groups.iter().position(|(b, _)| Arc::ptr_eq(b, bytes));
        let idx = pos.unwrap_or_else(|| {
            groups.push((Arc::clone(bytes), vec![]));
            groups.len() - 1
        });
        groups[idx].1.push((i, *page_num));
    }

    // --- 2. Load, renumber, and merge each source document ---
    let mut output = Document::with_version("1.5");
    let mut all_page_ids: Vec<ObjectId> = vec![(0, 0); requests.len()];
    let mut next_id: u32 = 1;

    for (bytes, page_requests) in &groups {
        let mut doc = Document::load_mem(bytes).map_err(|e| format!("Load error: {e}"))?;

        // Renumber all objects in this doc starting from next_id.
        // Updates all internal references and the /Root trailer entry
        // so that get_pages() works after renumbering.
        renumber(&mut doc, next_id);

        // Re-read page map after renumbering (references are now updated)
        let pages_map = doc.get_pages(); // BTreeMap<u32, ObjectId>
        for &(req_idx, page_num) in page_requests {
            let oid = pages_map
                .get(&page_num)
                .ok_or_else(|| format!("Page {page_num} not found in PDF"))?;
            all_page_ids[req_idx] = *oid;
        }

        next_id = doc.max_id + 1;
        // Move all objects from this doc into output
        let old = std::mem::take(&mut doc.objects);
        output.objects.extend(old);
    }
    output.max_id = next_id - 1;

    // --- 3. Build a new /Pages tree ---
    let kids: Vec<Object> = all_page_ids
        .iter()
        .map(|&id| Object::Reference(id))
        .collect();
    let count = kids.len() as i64;

    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
    pages_dict.set("Kids", Object::Array(kids));
    pages_dict.set("Count", Object::Integer(count));

    output.max_id += 1;
    let pages_id: ObjectId = (output.max_id, 0);
    output.objects.insert(pages_id, Object::Dictionary(pages_dict));

    // Update each page's /Parent to point to our new /Pages node
    for &page_id in &all_page_ids {
        match output.objects.get_mut(&page_id) {
            Some(Object::Dictionary(dict)) => {
                dict.set("Parent", Object::Reference(pages_id));
            }
            Some(Object::Stream(stream)) => {
                stream.dict.set("Parent", Object::Reference(pages_id));
            }
            _ => {}
        }
    }

    // --- 4. Build /Catalog and set trailer ---
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));

    output.max_id += 1;
    let catalog_id: ObjectId = (output.max_id, 0);
    output.objects.insert(catalog_id, Object::Dictionary(catalog));

    output.trailer.set("Root", Object::Reference(catalog_id));
    output.trailer.set("Size", Object::Integer((output.max_id + 1) as i64));

    // --- 5. Serialize ---
    let mut buf = Vec::new();
    output
        .save_to(&mut buf)
        .map_err(|e| format!("Save error: {e}"))?;
    Ok(buf)
}

/// Renumber all objects in `doc` sequentially starting from `start`.
/// Updates all internal object references and the /Root trailer entry.
fn renumber(doc: &mut Document, start: u32) {
    // Build old -> new ID mapping
    let mapping: HashMap<ObjectId, ObjectId> = doc
        .objects
        .keys()
        .copied()
        .enumerate()
        .map(|(i, old)| (old, (start + i as u32, 0)))
        .collect();

    // Rebuild objects map with new IDs and remapped references
    let old_objects = std::mem::take(&mut doc.objects);
    doc.objects = old_objects
        .into_iter()
        .map(|(old_id, mut obj)| {
            remap_refs(&mut obj, &mapping);
            (mapping[&old_id], obj)
        })
        .collect();

    // Remap /Root in trailer so get_pages() can follow it after renumbering
    if let Ok(Object::Reference(root_id)) = doc.trailer.get(b"Root") {
        let root_id = *root_id;
        if let Some(&new_root) = mapping.get(&root_id) {
            doc.trailer.set("Root", Object::Reference(new_root));
        }
    }

    doc.max_id = mapping.values().map(|id| id.0).max().unwrap_or(start);
}

fn remap_refs(obj: &mut Object, m: &HashMap<ObjectId, ObjectId>) {
    match obj {
        Object::Reference(id) => {
            if let Some(&new_id) = m.get(id) {
                *id = new_id;
            }
        }
        Object::Array(arr) => {
            for item in arr.iter_mut() {
                remap_refs(item, m);
            }
        }
        Object::Dictionary(dict) => {
            // Dictionary implements IntoIterator yielding (&[u8], &mut Object)
            // We iterate via the known lopdf API
            remap_dict(dict, m);
        }
        Object::Stream(stream) => {
            remap_dict(&mut stream.dict, m);
        }
        _ => {}
    }
}

fn remap_dict(dict: &mut Dictionary, m: &HashMap<ObjectId, ObjectId>) {
    // Collect keys first to avoid borrow conflict
    let keys: Vec<Vec<u8>> = dict.iter().map(|(k, _)| k.to_vec()).collect();
    for key in keys {
        if let Ok(val) = dict.get_mut(key.as_slice()) {
            remap_refs(val, m);
        }
    }
}
