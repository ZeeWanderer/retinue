#include "widget_capture.h"

#include <QAbstractItemModel>
#include <QAbstractItemView>
#include <QApplication>
#include <QSet>
#include <QVariant>
#include <QWidget>

#include <optional>

namespace {
constexpr auto TargetProperty = "wfguiCaptureTarget";
constexpr auto ContainsItemsProperty = "wfguiCaptureItems";
constexpr auto ItemProperty = "wfguiCaptureItem";

struct CaptureRegion {
  QWidget *surface;
  QRect rect;
  QWidget *clipFrom;
};

QString targetName(const QWidget *widget) {
  return widget->property(TargetProperty).toString();
}

QRect globalRect(const QWidget *widget, const QRect &rect) {
  return {widget->mapToGlobal(rect.topLeft()), rect.size()};
}

QRect clippedGlobalRect(const CaptureRegion &region, int padding) {
  QRect result = globalRect(region.surface, region.rect)
                     .adjusted(-padding, -padding, padding, padding);
  for (QWidget *clip = region.clipFrom; clip; clip = clip->parentWidget()) {
    result = result.intersected(globalRect(clip, clip->rect()));
    if (clip->isWindow()) {
      break;
    }
  }
  QWidget *window = region.surface->window();
  return result.intersected(globalRect(window, window->rect()));
}

bool regionVisible(const CaptureRegion &region) {
  return region.surface->isVisible() && region.surface->window()->isVisible() &&
         !clippedGlobalRect(region, 0).isEmpty();
}

bool regionFullyVisible(const CaptureRegion &region) {
  return clippedGlobalRect(region, 0) ==
         globalRect(region.surface, region.rect);
}

bool before(const CaptureRegion &left, const CaptureRegion &right) {
  const QRect leftRect = clippedGlobalRect(left, 0);
  const QRect rightRect = clippedGlobalRect(right, 0);
  return leftRect.top() != rightRect.top() ? leftRect.top() < rightRect.top()
                                           : leftRect.left() < rightRect.left();
}

std::optional<CaptureRegion> widgetRegion(const QString &name) {
  std::optional<CaptureRegion> best;
  for (QWidget *widget : QApplication::allWidgets()) {
    if (targetName(widget) != name) {
      continue;
    }
    CaptureRegion candidate{widget, widget->rect(), widget->parentWidget()};
    if (!regionVisible(candidate)) {
      continue;
    }
    if (!best || before(candidate, *best)) {
      best = candidate;
    }
  }
  return best;
}

std::optional<CaptureRegion> viewItemRegion(QAbstractItemView *view) {
  if (!view->model() || !view->viewport()) {
    return std::nullopt;
  }
  std::optional<CaptureRegion> best;
  std::optional<CaptureRegion> partial;
  const QModelIndex root = view->rootIndex();
  const int rows = view->model()->rowCount(root);
  for (int row = 0; row < rows; ++row) {
    QRect rect;
    const int columns = view->model()->columnCount(root);
    for (int column = 0; column < columns; ++column) {
      const QRect cell =
          view->visualRect(view->model()->index(row, column, root));
      if (cell.isValid()) {
        rect = rect.isValid() ? rect.united(cell) : cell;
      }
    }
    CaptureRegion candidate{view->viewport(), rect, view->viewport()};
    if (!rect.isValid() || !regionVisible(candidate)) {
      continue;
    }
    auto &target = regionFullyVisible(candidate) ? best : partial;
    if (!target || before(candidate, *target)) {
      target = candidate;
    }
  }
  return best ? best : partial;
}

std::optional<CaptureRegion> childItemRegion(QWidget *container) {
  std::optional<CaptureRegion> best;
  std::optional<CaptureRegion> partial;
  for (QWidget *widget : container->findChildren<QWidget *>()) {
    if (!widget->property(ItemProperty).toBool()) {
      continue;
    }
    CaptureRegion candidate{widget, widget->rect(), widget->parentWidget()};
    if (!regionVisible(candidate)) {
      continue;
    }
    auto &target = regionFullyVisible(candidate) ? best : partial;
    if (!target || before(candidate, *target)) {
      target = candidate;
    }
  }
  return best ? best : partial;
}

std::optional<CaptureRegion> itemRegion(const QString &containerName) {
  std::optional<CaptureRegion> best;
  for (QWidget *widget : QApplication::allWidgets()) {
    if (targetName(widget) != containerName) {
      continue;
    }
    std::optional<CaptureRegion> candidate;
    if (auto *view = qobject_cast<QAbstractItemView *>(widget)) {
      candidate = viewItemRegion(view);
    } else if (widget->property(ContainsItemsProperty).toBool()) {
      candidate = childItemRegion(widget);
    }
    if (candidate && (!best || before(*candidate, *best))) {
      best = candidate;
    }
  }
  return best;
}
} // namespace

namespace wfgui {

void setCaptureTarget(QWidget *widget, const QString &name,
                      bool containsItems) {
  widget->setProperty(TargetProperty, name);
  widget->setProperty(ContainsItemsProperty, containsItems);
}

void setCaptureItem(QWidget *widget) {
  widget->setProperty(ItemProperty, true);
}

QStringList captureTargetNames() {
  QSet<QString> names;
  for (QWidget *widget : QApplication::allWidgets()) {
    const QString name = targetName(widget);
    if (name.isEmpty()) {
      continue;
    }
    names.insert(name);
    if (qobject_cast<QAbstractItemView *>(widget) ||
        widget->property(ContainsItemsProperty).toBool()) {
      names.insert(name + ".item");
    }
  }
  QStringList result(names.cbegin(), names.cend());
  result.sort();
  return result;
}

QPixmap grabCaptureTarget(const QString &name, int padding, QString *error) {
  const bool item = name.endsWith(".item");
  const QString containerName = item ? name.chopped(5) : name;
  const std::optional<CaptureRegion> region =
      item ? itemRegion(containerName) : widgetRegion(name);
  if (!region) {
    if (error) {
      const QStringList known = captureTargetNames();
      *error = known.contains(name)
                   ? QString("capture target '%1' is not visible or has no "
                             "visible items")
                         .arg(name)
                   : QString("unknown capture target '%1'").arg(name);
    }
    return {};
  }

  const QRect global = clippedGlobalRect(*region, padding);
  QWidget *window = region->surface->window();
  const QRect local(window->mapFromGlobal(global.topLeft()), global.size());
  return window->grab(local);
}

} // namespace wfgui
