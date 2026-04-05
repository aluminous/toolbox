# PDF Rearrange — Product Specification

## Overview

A single-page web application for rearranging and concatenating PDF pages. All processing happens client-side; no files are ever transmitted to a server.

---

## Loading PDFs

- The user may load one or more PDF files at a time.
- Files may be selected via a file picker dialog.
- Files may be dragged from the operating system and dropped anywhere on the page.
- Multiple files may be loaded in any combination of the above methods.
- Additional files may be loaded at any time; new pages are appended to the existing source pool.

---

## Source Panel

- All pages from all loaded PDFs are displayed as thumbnails in a scrollable source panel.
- Each thumbnail shows a rendered preview of the page content.
- Each thumbnail is labelled with its source filename and page number.
- Thumbnails appear in file-load order, with pages within each file in document order.

---

## Page Selection

- Any number of source pages may be selected simultaneously.
- Clicking or tapping a page toggles its selection without affecting other selected pages.
- Shift+click extends the selection to a contiguous range from the last selected page.
- A "Select All" button selects all source pages at once.
- Selected pages are visually distinguished from unselected pages.

---

## Output Panel

- A separate output panel holds the pages that will form the saved PDF.
- Pages are added to the output panel by:
  - Dragging a thumbnail from the source panel and dropping it into the output panel.
  - Dragging any one of several selected source pages moves all selected pages together, in source order.
  - Clicking "Add Selected" to append all currently selected source pages in their displayed order.
- The same source page may be added to the output panel more than once.
- Pages from different source PDFs may be mixed freely.
- Pages in the output panel may be reordered by dragging.
- While dragging, the page thumbnail is shown as the drag image.
- Individual pages may be removed from the output panel.
- The output panel may be fully cleared.
- Drag interactions work on both desktop (mouse) and mobile (touch); on touch, a floating preview follows the finger.

---

## Saving

- Clicking "Save PDF" downloads a new PDF file to the user's device.
- The saved file contains exactly the pages in the output panel, in the order they appear.
- Page content (text, images, fonts, annotations) is preserved faithfully from the source files.
- The download begins immediately with no server round-trip.

---

## Privacy & Security

- No file data is transmitted over the network at any point.
- All PDF parsing, processing, and generation occurs entirely within the browser.
