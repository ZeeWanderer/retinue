#include "thumbnail_widget.h"

#include <QPainter>
#include <QPixmapCache>
#include <QStyleOption>

#include "image_cache.h"

ThumbnailWidget::ThumbnailWidget(QWidget *parent) : QWidget(parent) {}

void ThumbnailWidget::setAsset(const wfgui::AssetRef &asset) {
  if (asset_ == asset) {
    return;
  }
  asset_ = asset;
  update();
}

void ThumbnailWidget::setImageBounds(QSize bounds) {
  if (bounds_ == bounds) {
    return;
  }
  bounds_ = bounds;
  update();
}

void ThumbnailWidget::paintEvent(QPaintEvent *) {
  QPainter painter(this);
  QStyleOption option;
  option.initFrom(this);
  style()->drawPrimitive(QStyle::PE_Widget, &option, &painter, this);
  const QSize bounds = bounds_.isValid() ? bounds_.boundedTo(size()) : size();
  QPixmap image = wfgui::cachedThumbnail(painter, asset_, bounds);
  if (!image.isNull() && tint_.isValid()) {
    const QString key =
        QString("wfgui-tint:%1:%2").arg(image.cacheKey()).arg(tint_.rgba());
    QPixmap tinted;
    if (!QPixmapCache::find(key, &tinted)) {
      tinted = image.copy();
      QPainter mask(&tinted);
      mask.setCompositionMode(QPainter::CompositionMode_SourceIn);
      mask.fillRect(QRectF(QPointF{}, image.deviceIndependentSize()), tint_);
      mask.end();
      QPixmapCache::insert(key, tinted);
    }
    image = std::move(tinted);
  }
  wfgui::drawContained(painter, rect(), image);
}

void ThumbnailWidget::setTint(QColor color) {
  if (tint_ != color) {
    tint_ = color;
    update();
  }
}
