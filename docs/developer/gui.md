# Qt GUI

Reference implementation in `apps/wfgui/qt/`. New desktop work belongs in the
[Rust client](native-ui.md); do not maintain both implementations in parallel.

Use Qt model/view for dense or unbounded collections:

- Keep records in `QAbstractItemModel` implementations and render them with a
  `QListView` plus `QStyledItemDelegate`. Do not create one child widget per record.
- Put card geometry in a layout helper shared by painting, tooltips, and hit testing.
- Keep disk, network, image decoding, and expensive transformation out of `paint()`
  and model `data()` calls. Create `QPixmap` objects only on the GUI thread.
- Size thumbnails for the paint device's DPR. QPainter target geometry is logical;
  pixmap source rectangles use physical pixels (`pixmap.rect()`). Test fractional DPR.
- Use `ThumbnailWidget` for image-only widgets and `cachedThumbnail` in delegates.
  Both share async decode, derivative identity and DPR-aware caching.
- Complete asynchronous work with `update(itemRect)`. Repaint the whole viewport only
  when shared visual state changes. Use `update()`, not synchronous `repaint()`.
- Emit role-specific `dataChanged` ranges. Reset a model only when row identity or
  structure changes, and preserve the visible anchor across reordering.
- Use uniform item sizes and `QListView::SinglePass` where card geometry allows it.
  Change layout mode or widget paint attributes only after measuring the result.

Bounded widget compositions, including Market orders, retain children by stable
identity and update fields in place. Do not rebuild controls when an image or
quote arrives. Copy an action callback before invoking it if synchronous model
updates can replace that callback.

Thumbnail work admits at most three decodes, with a 32 MiB output reservation
budget (one larger image may run alone). The result stays admitted until the GUI
consumes it. Queued derivative writes have a separate 32 MiB memory budget;
optional writes may be skipped under pressure. Neither budget caps disk storage.
Keep source registration FIFO and ahead of derivative writes. Linux image workers
use nice 5 or lower scheduling priority; the GUI thread's priority is unchanged.

Socket input uses bounded reads and yields after eight frames or 4 ms of dispatch.
A single frame is limited to 64 MiB. Reply deadlines use the existing reconnect
path: reads may retry; mutations with unknown outcomes are reconciled, not replayed.
Build planning has a longer deadline than ordinary reads.

Qt coalesces `update()` calls and clips paint events to dirty regions. Delegates add
application drawing while the view retains virtualization, scrolling, and backing-store
behavior. See the Qt documentation for
[`QAbstractItemView`](https://doc.qt.io/qt-6/qabstractitemview.html),
[`QListView`](https://doc.qt.io/qt-6/qlistview.html), and
[`QWidget` painting](https://doc.qt.io/qt-6/qwidget.html#update).
