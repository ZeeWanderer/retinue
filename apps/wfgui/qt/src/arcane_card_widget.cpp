#include "arcane_card_widget.h"

#include <QFontMetrics>
#include <QHash>
#include <QPainter>

#include <algorithm>
#include <utility>

#include "app_controller.h"
#include "image_cache.h"

namespace {
constexpr qreal kCardWidth = 160.0;
constexpr qreal kCardHeight = 108.0;
constexpr qreal kFrameSize = 180.0;
constexpr int kWidgetWidth = 160;
constexpr int kWidgetHeight = 108;

QString pathName(const QString &path) {
  const qsizetype slash = path.lastIndexOf('/');
  return slash >= 0 ? path.mid(slash + 1) : path;
}

QString arcaneName(const QJsonObject &upgrade) {
  const QString name = upgrade.value("name").toString();
  return name.isEmpty() ? pathName(upgrade.value("item_type").toString())
                        : name;
}

QString arcaneRarity(const QJsonObject &upgrade) {
  const QString rarity = upgrade.value("rarity").toString().toLower();
  if (rarity == "uncommon" || rarity == "rare" || rarity == "legendary") {
    return rarity;
  }
  return "common";
}

QPixmap resourcePixmap(const QString &path) {
  static QHash<QString, QPixmap> cache;
  const auto found = cache.constFind(path);
  if (found != cache.cend()) {
    return *found;
  }
  return cache.insert(path, QPixmap(path)).value();
}

void drawPixmap(QPainter &painter, const QRectF &destination,
                const QPixmap &pixmap) {
  if (!pixmap.isNull()) {
    painter.drawPixmap(destination, pixmap,
                       QRectF(QPointF{}, pixmap.deviceIndependentSize()));
  }
}

QRectF mapped(const QRectF &root, qreal scale, qreal x, qreal y, qreal width,
              qreal height) {
  return {root.left() + x * scale, root.top() + y * scale, width * scale,
          height * scale};
}

} // namespace

namespace wfgui {

ArcaneCardWidget::ArcaneCardWidget(AppController *controller, QJsonObject slot,
                                   QJsonObject upgrade, QWidget *parent)
    : QWidget(parent), controller_(controller), slot_(std::move(slot)),
      upgrade_(std::move(upgrade)),
      id_(upgrade_.value("asset").toObject().value("id").toString()),
      name_(arcaneName(upgrade_)), rarity_(arcaneRarity(upgrade_)) {
  setObjectName("buildArcaneCard");
  setProperty("slotId", slot_.value("id").toString());
  setProperty("upgradeName", name_);
  setProperty("rarity", rarity_);
  setProperty("empty", upgrade_.isEmpty());
  setProperty("unlocked", slot_.value("unlocked").toBool(true));
  setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Fixed);
  setFixedSize(kWidgetWidth, kWidgetHeight);

  QStringList tooltip;
  if (upgrade_.isEmpty()) {
    tooltip.append(slot_.value("label").toString("Arcane"));
    tooltip.append(slot_.value("unlocked").toBool(true) ? "Empty" : "Locked");
  } else {
    tooltip.append(name_);
    if (upgrade_.value("rank").isDouble()) {
      tooltip.append(QString("Rank %1").arg(upgrade_.value("rank").toInt()));
    }
  }
  setToolTip(tooltip.join('\n'));

  if (controller_ && !id_.isEmpty()) {
    connect(controller_, &AppController::assetsChanged, this,
            [this](const QStringList &ids) {
              if (ids.contains(id_)) {
                update();
              }
            });
  }
}

void ArcaneCardWidget::paintEvent(QPaintEvent *) {
  QPainter painter(this);
  painter.setRenderHints(QPainter::Antialiasing | QPainter::TextAntialiasing |
                         QPainter::SmoothPixmapTransform);
  if (upgrade_.isEmpty()) {
    paintEmpty(painter);
  } else {
    paintArcane(painter);
  }
}

void ArcaneCardWidget::paintArcane(QPainter &painter) const {
  const qreal scale = std::min(width() / kCardWidth, height() / kCardHeight);
  const QRectF root((width() - kCardWidth * scale) / 2.0,
                    (height() - kCardHeight * scale) / 2.0, kCardWidth * scale,
                    kCardHeight * scale);
  const QRectF frameBounds =
      mapped(root, scale, -10.0, -36.0, kFrameSize, kFrameSize);
  const QRectF artBounds = mapped(root, scale, 20.0, 20.0, 120.0, 68.0);
  drawPixmap(painter, frameBounds,
             resourcePixmap(":/resources/arcane-frames/" + rarity_ + ".png"));

  const AssetRef asset = controller_ ? controller_->assetRef(id_) : AssetRef{};
  const QPixmap art = cachedThumbnail(painter, asset, artBounds.size().toSize(),
                                      artBounds.toAlignedRect());
  drawContained(painter, artBounds, art);

  QFont nameFont = font();
  nameFont.setPixelSize(std::max(10, qRound(14.0 * scale)));
  nameFont.setWeight(QFont::Bold);
  painter.setFont(nameFont);
  painter.setPen(QColor("#cccccc"));
  const QRectF textBounds = mapped(root, scale, 0.0, 80.0, 160.0, 28.0);
  const QString text = QFontMetrics(nameFont).elidedText(
      name_, Qt::ElideRight, qRound(textBounds.width()));
  painter.drawText(textBounds, Qt::AlignCenter, text);
}

void ArcaneCardWidget::paintEmpty(QPainter &painter) const {
  const qreal scale = std::min(width() / kCardWidth, height() / kCardHeight);
  const QRectF root((width() - kCardWidth * scale) / 2.0,
                    (height() - kCardHeight * scale) / 2.0, kCardWidth * scale,
                    kCardHeight * scale);
  painter.save();
  painter.setOpacity(slot_.value("unlocked").toBool(true) ? 0.4 : 0.2);
  const QRectF bounds = mapped(root, scale, 30.0, 15.0, 100.0, 60.0);
  drawPixmap(painter, bounds,
             resourcePixmap(":/resources/arcane-frames/empty.png"));
  painter.restore();
}

} // namespace wfgui
