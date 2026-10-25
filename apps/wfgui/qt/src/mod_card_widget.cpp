#include "mod_card_widget.h"

#include <QAbstractScrollArea>
#include <QEasingCurve>
#include <QEnterEvent>
#include <QFontMetrics>
#include <QHash>
#include <QHideEvent>
#include <QImage>
#include <QJsonArray>
#include <QLinearGradient>
#include <QPainter>
#include <QPainterPath>
#include <QRegularExpression>
#include <QScrollBar>
#include <QTransform>
#include <QVariantAnimation>

#include <algorithm>
#include <cmath>
#include <utility>

#include "app_controller.h"
#include "image_cache.h"

namespace {
constexpr qreal kCardWidth = 184.0;
constexpr qreal kCardHeight = 90.0;
constexpr qreal kExpandedCardHeight = 260.0;
constexpr qreal kExpandedScale = 1.1;
constexpr int kWidgetWidth = 200;
constexpr int kWidgetHeight = 110;
constexpr int kExpandedWidgetWidth = 220;
constexpr int kExpandedWidgetHeight = 308;
constexpr int kPreviewDurationMs = 120;
constexpr qreal kExpandedTextBottom = 215.0;
constexpr qreal kExpandedTextGap = 2.5;
constexpr qreal kExpandedArtworkGap = 5.0;

struct FrameProfile {
  QString top;
  QString bottom;
  QString background;
  QString sideLight;
  QString lowerTab;
  QColor textColor;
  qreal topWidth = 184.0;
  qreal topY = -10.0;
  qreal topClipHeight = 50.0;
  qreal bottomWidth = 184.0;
  qreal bottomY = 10.0;
  qreal bottomClipHeight = 80.0;
  qreal contentX = 0.0;
  qreal contentWidth = 184.0;
  qreal contentHeight = 70.0;
  qreal artworkX = 5.0;
  qreal artworkY = 3.0;
  qreal artworkWidth = 174.0;
  qreal artworkHeight = 67.0;
  qreal drainRight = 4.0;
  qreal drainTop = 8.0;
  qreal nameX = 12.0;
  qreal nameWidth = 160.0;
  qreal rankWidth = 117.0;
  qreal rankBottom = 2.0;
  qreal sideTop = 24.0;
};

struct ExpandedTextLayout {
  qreal artworkHeight;
  qreal nameY;
  qreal nameHeight;
  qreal effectsY;
  qreal effectsHeight;
};

QString pathName(const QString &path) {
  const qsizetype slash = path.lastIndexOf('/');
  return slash >= 0 ? path.mid(slash + 1) : path;
}

QString upgradeName(const QJsonObject &upgrade) {
  const QString name = upgrade.value("name").toString();
  return name.isEmpty() ? pathName(upgrade.value("item_type").toString())
                        : name;
}

QString upgradeMeta(const QJsonObject &upgrade) {
  QStringList parts;
  if (upgrade.value("rank").isDouble()) {
    const int rank = upgrade.value("rank").toInt();
    const int maximum = upgrade.value("max_rank").toInt(-1);
    parts.append(maximum >= rank ? QString("Rank %1/%2").arg(rank).arg(maximum)
                                 : QString("Rank %1").arg(rank));
  }
  if (upgrade.value("drain").isDouble()) {
    const int drain = upgrade.value("drain").toInt();
    const int effective = upgrade.value("effective_drain").toInt(drain);
    parts.append(effective == drain
                     ? QString("Drain %1").arg(drain)
                     : QString("Drain %1 (%2 base)").arg(effective).arg(drain));
  }
  return parts.join("  ·  ");
}

QString modPolarity(const QJsonObject &upgrade) {
  const QString polarity = upgrade.value("polarity").toString();
  return polarity.isEmpty() ? upgrade.value("mod_polarity").toString()
                            : polarity;
}

QString polarityResource(const QString &polarity) {
  const QString name = polarity == "omni"     ? "universal"
                       : polarity == "umbral" ? "umbra"
                                              : polarity;
  return name.isEmpty() || name == "none" || name == "unknown"
             ? QString{}
             : QString(":/resources/polarities/%1.png").arg(name);
}

QColor polarityColor(const QString &state) {
  if (state == "matched") {
    return QColor("#68ce5f");
  }
  if (state == "mismatched") {
    return QColor("#f73533");
  }
  if (state == "unknown") {
    return QColor("#a9afc0");
  }
  return QColor("#ffffff");
}

int effectiveDrain(const QJsonObject &upgrade) {
  for (const char *key : {"effective_drain", "drain", "cost"}) {
    if (upgrade.value(key).isDouble()) {
      return upgrade.value(key).toInt();
    }
  }
  return -1;
}

QString resource(const QString &name) {
  return ":/resources/mod-frames/" + name;
}

FrameProfile standardProfile(const QString &rarity) {
  FrameProfile profile;
  if (rarity == "legendary") {
    profile.top = resource("LegendaryFrameTop.png");
    profile.bottom = resource("LegendaryFrameBottom.png");
    profile.background = resource("LegendaryBackground.png");
    profile.sideLight = resource("LegendarySideLight.png");
    profile.lowerTab = resource("LegendaryLowerTab.png");
    profile.textColor = QColor("#dfdfdf");
    profile.contentHeight = 75.0;
    profile.artworkHeight = 72.0;
    profile.sideTop = 11.0;
    return profile;
  }
  if (rarity == "rare") {
    profile.top = resource("GoldFrameTop.png");
    profile.bottom = resource("GoldFrameBottom.png");
    profile.background = resource("GoldBackground.png");
    profile.sideLight = resource("GoldSideLight.png");
    profile.lowerTab = resource("GoldLowerTab.png");
    profile.textColor = QColor("#fbecc4");
  } else if (rarity == "uncommon") {
    profile.top = resource("SilverFrameTop.png");
    profile.bottom = resource("SilverFrameBottom.png");
    profile.background = resource("SilverBackground.png");
    profile.sideLight = resource("SilverSideLight.png");
    profile.lowerTab = resource("SilverLowerTab.png");
    profile.textColor = QColor("#bec0c2");
  } else {
    profile.top = resource("BronzeFrameTop.png");
    profile.bottom = resource("BronzeFrameBottom.png");
    profile.background = resource("BronzeBackground.png");
    profile.sideLight = resource("BronzeSideLight.png");
    profile.lowerTab = resource("BronzeLowerTab.png");
    profile.textColor = QColor("#c79989");
  }
  return profile;
}

FrameProfile specialProfile(const QString &variant) {
  FrameProfile profile;
  profile.textColor = QColor("#dfdfdf");
  profile.topClipHeight = 90.0;
  profile.bottomWidth = 200.0;
  profile.bottomClipHeight = 85.0;
  profile.contentX = 5.0;
  profile.contentWidth = 174.0;
  profile.artworkX = 10.0;
  profile.artworkWidth = 164.0;
  profile.rankWidth = 106.0;
  profile.rankBottom = 0.0;
  profile.nameX = 16.0;
  profile.nameWidth = 152.0;

  if (variant == "galvanized") {
    profile.top = resource("GalvanizedFrameTop.png");
    profile.bottom = resource("GalvanizedFrameBottom.png");
    profile.background = resource("LegendaryBackground.png");
    profile.topWidth = 200.0;
    profile.drainRight = 13.0;
    profile.drainTop = 11.0;
    profile.nameX = 18.0;
    profile.nameWidth = 148.0;
    profile.sideLight = resource("LegendarySideLight.png");
    profile.lowerTab = resource("LegendaryLowerTab.png");
    profile.sideTop = 21.0;
  } else if (variant == "amalgam") {
    profile.top = resource("AmalgamFrameTop.png");
    profile.bottom = resource("AmalgamFrameBottom.png");
    profile.background = resource("AmalgamBackground.png");
    profile.drainRight = 10.0;
    profile.sideLight = resource("LegendarySideLight.png");
    profile.lowerTab = resource("LegendaryLowerTab.png");
    profile.sideTop = 21.0;
  } else {
    profile.top = resource("OmegaFrameTop.png");
    profile.bottom = resource("OmegaFrameBottom.png");
    profile.background = resource("SilverBackground.png");
    profile.textColor = QColor("#a783c8");
    profile.drainRight = 15.0;
    profile.rankBottom = 3.0;
    profile.sideLight = resource("OmegaSideLight.png");
    profile.lowerTab = resource("RivenLowerTab.png");
  }
  return profile;
}

FrameProfile frameProfile(const QJsonObject &upgrade) {
  const QString variant =
      upgrade.value("mod_variant").toString("standard").toLower();
  if (variant == "galvanized" || variant == "amalgam" || variant == "riven") {
    return specialProfile(variant);
  }
  return standardProfile(upgrade.value("rarity").toString().toLower());
}

QPixmap staticPixmap(const QString &path) {
  static QHash<QString, QPixmap> cache;
  const auto found = cache.constFind(path);
  if (found != cache.cend()) {
    return *found;
  }
  return cache.insert(path, QPixmap(path)).value();
}

QPixmap mirroredPixmap(const QString &path) {
  static QHash<QString, QPixmap> cache;
  const auto found = cache.constFind(path);
  if (found != cache.cend()) {
    return *found;
  }
  return cache
      .insert(path,
              staticPixmap(path).transformed(QTransform().scale(-1.0, 1.0)))
      .value();
}

qreal mix(qreal from, qreal to, qreal progress) {
  return from + (to - from) * progress;
}

QRect interpolate(const QRect &from, const QRect &to, qreal progress) {
  return {qRound(mix(from.x(), to.x(), progress)),
          qRound(mix(from.y(), to.y(), progress)),
          qRound(mix(from.width(), to.width(), progress)),
          qRound(mix(from.height(), to.height(), progress))};
}

QString cleanStat(QString value) {
  static const QRegularExpression Markup("<[^>]+>");
  value.remove(Markup);
  value.replace("\r\n", "\n");
  value.replace('\r', '\n');
  return value.trimmed();
}

QString effectText(const QJsonObject &upgrade) {
  QStringList effects;
  for (const QJsonValue &value : upgrade.value("effects").toArray()) {
    const QString effect = cleanStat(value.toString());
    if (!effect.isEmpty()) {
      effects.append(effect);
    }
  }
  return effects.join('\n');
}

qreal wrappedTextHeight(const QFont &font, qreal width, const QString &text) {
  if (text.isEmpty()) {
    return 0.0;
  }
  const QRectF bounds = QFontMetricsF(font).boundingRect(
      QRectF(0.0, 0.0, width, 1000.0),
      Qt::AlignHCenter | Qt::AlignTop | Qt::TextWordWrap, text);
  return std::ceil(bounds.height());
}

ExpandedTextLayout expandedTextLayout(const QFont &baseFont,
                                      const FrameProfile &profile,
                                      const QString &name,
                                      const QString &effects) {
  QFont nameFont = baseFont;
  nameFont.setPixelSize(qRound(17.0 * kExpandedScale));
  nameFont.setWeight(QFont::Medium);
  QFont effectsFont = baseFont;
  effectsFont.setPixelSize(qRound(13.0 * kExpandedScale));

  const qreal width = profile.nameWidth * kExpandedScale;
  const qreal nameHeight =
      wrappedTextHeight(nameFont, width, name) / kExpandedScale;
  const qreal effectsHeight =
      wrappedTextHeight(effectsFont, width, effects) / kExpandedScale;
  const qreal effectsY = kExpandedTextBottom - effectsHeight;
  const qreal nameY = effects.isEmpty()
                          ? kExpandedTextBottom - nameHeight
                          : effectsY - kExpandedTextGap - nameHeight;
  const qreal artworkHeight =
      std::clamp(nameY - kExpandedArtworkGap - profile.artworkY, 20.0, 174.0);
  return {artworkHeight, nameY, nameHeight, effectsY, effectsHeight};
}

QRectF mapped(const QRectF &root, qreal scale, qreal x, qreal y, qreal width,
              qreal height) {
  return {root.left() + x * scale, root.top() + y * scale, width * scale,
          height * scale};
}

void drawCovered(QPainter &painter, const QRectF &bounds,
                 const QPixmap &image) {
  if (image.isNull()) {
    return;
  }
  const QSizeF source = image.deviceIndependentSize();
  const qreal scale = std::max(bounds.width() / source.width(),
                               bounds.height() / source.height());
  const QSizeF target(source.width() * scale, source.height() * scale);
  const QRectF destination(bounds.center().x() - target.width() / 2.0,
                           bounds.center().y() - target.height() / 2.0,
                           target.width(), target.height());
  painter.drawPixmap(destination, image, QRectF(image.rect()));
}

void drawFrameLayer(QPainter &painter, const QRectF &root, qreal scale,
                    const QString &path, qreal width, qreal y,
                    qreal clipHeight) {
  const QPixmap image = staticPixmap(path);
  if (image.isNull()) {
    return;
  }
  const QSizeF source = image.deviceIndependentSize();
  const qreal targetWidth = width * scale;
  const qreal targetHeight = source.height() / source.width() * targetWidth;
  const QRectF destination(root.center().x() - targetWidth / 2.0,
                           root.top() + y * scale, targetWidth, targetHeight);
  painter.save();
  painter.setClipRect(
      mapped(root, scale, (kCardWidth - width) / 2.0, y, width, clipHeight));
  painter.drawPixmap(destination, image, QRectF(QPointF{}, source));
  painter.restore();
}

void drawSideLights(QPainter &painter, const QRectF &root, qreal scale,
                    const FrameProfile &profile, qreal expansion) {
  if (profile.sideLight.isEmpty() || expansion <= 0.0) {
    return;
  }
  const QPixmap rightImage = staticPixmap(profile.sideLight);
  const QPixmap leftImage = mirroredPixmap(profile.sideLight);
  if (rightImage.isNull()) {
    return;
  }
  const qreal width = 8.0 * scale;
  const qreal height = 182.0 * scale * expansion;
  const qreal top = root.top() + profile.sideTop * scale;
  const QRectF left(root.left() + 2.0 * scale, top, width, height);
  const QRectF right(root.right() - 2.0 * scale - width, top, width, height);
  painter.save();
  painter.setOpacity(expansion);
  painter.drawPixmap(left, leftImage,
                     QRectF(QPointF{}, leftImage.deviceIndependentSize()));
  painter.drawPixmap(right, rightImage,
                     QRectF(QPointF{}, rightImage.deviceIndependentSize()));
  painter.restore();
}

void drawCompatibility(QPainter &painter, const QRectF &root, qreal scale,
                       qreal cardHeight, const FrameProfile &profile,
                       const QJsonObject &upgrade, qreal expansion) {
  const QString compatibility = upgrade.value("compat_name").toString();
  if (compatibility.isEmpty() || profile.lowerTab.isEmpty() ||
      expansion <= 0.0) {
    return;
  }
  const QPixmap tab = staticPixmap(profile.lowerTab);
  const qreal width = 154.0 * scale;
  const qreal height = 20.0 * scale;
  const QRectF bounds(root.center().x() - width / 2.0,
                      root.top() + (cardHeight - 40.0) * scale, width, height);
  painter.save();
  painter.setOpacity(expansion);
  painter.drawPixmap(bounds, tab,
                     QRectF(QPointF{}, tab.deviceIndependentSize()));
  QFont font = painter.font();
  font.setPixelSize(std::max(9, qRound(12.0 * scale)));
  font.setWeight(QFont::Bold);
  painter.setFont(font);
  painter.setPen(profile.textColor);
  painter.drawText(bounds.translated(0.0, 1.0 * scale), Qt::AlignCenter,
                   compatibility.toUpper());
  painter.restore();
}

void drawDrain(QPainter &painter, const QRectF &root, qreal scale,
               const FrameProfile &profile, const QJsonObject &upgrade,
               const QString &polarity, const QString &state) {
  const int installed = effectiveDrain(upgrade);
  if (installed < 0) {
    return;
  }
  const QString drain = QString::number(installed);
  QFont font = painter.font();
  font.setPixelSize(std::max(9, qRound(12.0 * scale)));
  font.setWeight(QFont::Medium);
  painter.setFont(font);

  const QColor tint = polarityColor(state);
  const QPixmap glyph = wfgui::modPolarityPixmap(polarity, tint);
  const qreal glyphEdge = glyph.isNull() ? 0.0 : 13.0 * scale;
  const qreal gap = glyph.isNull() ? 0.0 : 2.0 * scale;
  const qreal textWidth = QFontMetricsF(font).horizontalAdvance(drain);
  const qreal contentWidth = textWidth + gap + glyphEdge;
  const qreal height = 15.0 * scale;
  const qreal right = root.right() - profile.drainRight * scale;
  const qreal top = root.top() + profile.drainTop * scale;
  const qreal x = right - contentWidth - 6.0 * scale;
  const qreal notch = 6.0 * scale;

  QPainterPath shape;
  shape.moveTo(x, top);
  shape.lineTo(right, top);
  shape.lineTo(right, top + height);
  shape.lineTo(x, top + height);
  shape.lineTo(x - notch, top + height - notch);
  shape.closeSubpath();
  painter.setBrush(QColor(0, 0, 0, 155));
  painter.setPen(QPen(profile.textColor, std::max(1.0, scale)));
  painter.drawPath(shape);

  const qreal contentLeft = x + 3.0 * scale;
  painter.setPen(tint);
  painter.drawText(QRectF(contentLeft, top, textWidth, height), Qt::AlignCenter,
                   drain);
  if (!glyph.isNull()) {
    const QRectF glyphBounds(contentLeft + textWidth + gap,
                             top + (height - glyphEdge) / 2.0, glyphEdge,
                             glyphEdge);
    painter.drawPixmap(glyphBounds, glyph,
                       QRectF(QPointF{}, glyph.deviceIndependentSize()));
  }
}

} // namespace

namespace wfgui {

QPixmap modPolarityPixmap(const QString &polarity, const QColor &color) {
  const QString path = polarityResource(polarity);
  if (path.isEmpty()) {
    return {};
  }
  static QHash<QString, QPixmap> cache;
  const QString key = path + color.name(QColor::HexArgb);
  const auto found = cache.constFind(key);
  if (found != cache.cend()) {
    return *found;
  }
  QImage image(path);
  image = image.convertToFormat(QImage::Format_ARGB32_Premultiplied);
  QPainter tint(&image);
  tint.setCompositionMode(QPainter::CompositionMode_SourceIn);
  tint.fillRect(image.rect(), color);
  tint.end();
  return cache.insert(key, QPixmap::fromImage(image)).value();
}

class ModCardPreview final : public QWidget {
public:
  ModCardPreview(ModCardWidget *source, QWidget *parent)
      : QWidget(parent), source_(source) {
    setObjectName("buildModCardPreview");
    setAttribute(Qt::WA_TransparentForMouseEvents);
    setAttribute(Qt::WA_NoSystemBackground);
    setAttribute(Qt::WA_TranslucentBackground);
  }

  void setBounds(const QRect &compact, const QRect &expanded) {
    compact_ = compact;
    expanded_ = expanded;
    setExpansion(expansion_);
  }

  qreal expansion() const { return expansion_; }

  void setExpansion(qreal expansion) {
    expansion_ = qBound(0.0, expansion, 1.0);
    setGeometry(interpolate(compact_, expanded_, expansion_));
    update();
  }

protected:
  void paintEvent(QPaintEvent *) override {
    if (!source_) {
      return;
    }
    QPainter painter(this);
    painter.setRenderHints(QPainter::Antialiasing | QPainter::TextAntialiasing |
                           QPainter::SmoothPixmapTransform);
    source_->paintCard(painter, expansion_, size());
  }

private:
  QPointer<ModCardWidget> source_;
  QRect compact_;
  QRect expanded_;
  qreal expansion_ = 0.0;
};

ModCardWidget::ModCardWidget(AppController *controller, QJsonObject slot,
                             QJsonObject upgrade, QString slotPolarity,
                             QWidget *parent)
    : QWidget(parent), controller_(controller), slot_(std::move(slot)),
      upgrade_(std::move(upgrade)), role_(slot_.value("role").toString("mod")),
      slotPolarity_(upgrade_.value("slot_polarity").toString(slotPolarity)),
      modPolarity_(modPolarity(upgrade_)),
      polarityState_(upgrade_.value("polarity_state").toString("neutral")),
      id_(upgrade_.value("asset").toObject().value("id").toString()),
      previewAnimation_(new QVariantAnimation(this)) {
  const QString variant =
      upgrade_.value("mod_variant").toString("standard").toLower();
  setObjectName("buildModCard");
  setProperty("slotId", slot_.value("id").toString());
  setProperty("upgradeName", upgradeName(upgrade_));
  setProperty("empty", upgrade_.isEmpty());
  setProperty("role", role_);
  setProperty("rarity", upgrade_.value("rarity").toString().toLower());
  setProperty("modVariant", variant);
  setProperty("modPolarity", modPolarity_);
  setProperty("slotPolarity", slotPolarity_);
  setProperty("polarityState", polarityState_);
  setProperty("effectiveDrain", effectiveDrain(upgrade_));
  setProperty("showsExternalPolarity", showsExternalPolarity());
  setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Fixed);
  setFixedSize(kWidgetWidth, kWidgetHeight);

  previewAnimation_->setDuration(kPreviewDurationMs);
  previewAnimation_->setEasingCurve(QEasingCurve::InOutQuad);
  connect(previewAnimation_, &QVariantAnimation::valueChanged, this,
          [this](const QVariant &value) {
            if (preview_) {
              preview_->setExpansion(value.toReal());
            }
          });
  connect(previewAnimation_, &QVariantAnimation::finished, this, [this] {
    if (!preview_ || previewAnimation_->endValue().toReal() > 0.0) {
      return;
    }
    preview_->deleteLater();
    preview_ = nullptr;
    update();
  });

  QStringList tooltip;
  if (upgrade_.isEmpty()) {
    tooltip.append(slot_.value("label").toString("Mod slot"));
    tooltip.append(slot_.value("unlocked").toBool(true) ? "Empty" : "Locked");
  } else {
    tooltip.append(upgradeName(upgrade_));
    const QString meta = upgradeMeta(upgrade_);
    if (!meta.isEmpty()) {
      tooltip.append(meta);
    }
  }
  if (!modPolarity_.isEmpty() && modPolarity_ != "none") {
    tooltip.append("Mod polarity: " + modPolarity_);
  }
  if (!slotPolarity_.isEmpty() && slotPolarity_ != "none") {
    tooltip.append("Slot polarity: " + slotPolarity_ + " (" + polarityState_ +
                   ")");
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

ModCardWidget::~ModCardWidget() { closePreview(false); }

void ModCardWidget::enterEvent(QEnterEvent *event) {
  QWidget::enterEvent(event);
  openPreview();
}

void ModCardWidget::leaveEvent(QEvent *event) {
  QWidget::leaveEvent(event);
  closePreview(true);
}

void ModCardWidget::hideEvent(QHideEvent *event) {
  closePreview(false);
  QWidget::hideEvent(event);
}

bool ModCardWidget::eventFilter(QObject *watched, QEvent *event) {
  if (preview_) {
    switch (event->type()) {
    case QEvent::WindowDeactivate:
    case QEvent::Hide:
    case QEvent::Close:
    case QEvent::Resize:
    case QEvent::Wheel:
    case QEvent::MouseButtonPress:
      closePreview(false);
      break;
    default:
      break;
    }
  }
  return QWidget::eventFilter(watched, event);
}

void ModCardWidget::openPreview() {
  if (upgrade_.isEmpty() || !isVisible()) {
    return;
  }
  if (preview_) {
    animatePreview(1.0);
    return;
  }
  QWidget *host = window();
  if (!host) {
    return;
  }
  const QRect compact(mapTo(host, QPoint{}), size());
  QRect expanded(QPoint{}, QSize(kExpandedWidgetWidth, kExpandedWidgetHeight));
  expanded.moveCenter(compact.center());

  preview_ = new ModCardPreview(this, host);
  preview_->setBounds(compact, expanded);
  preview_->show();
  preview_->raise();
  host->installEventFilter(this);
  for (QWidget *ancestor = parentWidget(); ancestor;
       ancestor = ancestor->parentWidget()) {
    if (auto *scroll = qobject_cast<QAbstractScrollArea *>(ancestor)) {
      scroll->viewport()->installEventFilter(this);
      scroll->verticalScrollBar()->installEventFilter(this);
      scroll->horizontalScrollBar()->installEventFilter(this);
      break;
    }
  }
  update();
  animatePreview(1.0);
}

void ModCardWidget::closePreview(bool animated) {
  if (!preview_) {
    return;
  }
  if (animated && preview_->expansion() > 0.0) {
    animatePreview(0.0);
    return;
  }
  previewAnimation_->stop();
  delete preview_;
  preview_ = nullptr;
  update();
}

void ModCardWidget::animatePreview(qreal target) {
  if (!preview_) {
    return;
  }
  previewAnimation_->stop();
  previewAnimation_->setStartValue(preview_->expansion());
  previewAnimation_->setEndValue(target);
  previewAnimation_->start();
}

void ModCardWidget::paintEvent(QPaintEvent *) {
  if (preview_) {
    return;
  }
  QPainter painter(this);
  painter.setRenderHints(QPainter::Antialiasing | QPainter::TextAntialiasing |
                         QPainter::SmoothPixmapTransform);
  paintCard(painter, 0.0, size());
}

void ModCardWidget::paintCard(QPainter &painter, qreal expansion,
                              const QSizeF &canvasSize) const {
  expansion = qBound(0.0, expansion, 1.0);
  const qreal scale = mix(1.0, kExpandedScale, expansion);
  const qreal cardHeight = mix(kCardHeight, kExpandedCardHeight, expansion);
  const qreal rootHeight = cardHeight * scale;
  const qreal freeHeight = std::max(0.0, canvasSize.height() - rootHeight);
  const QRectF root((canvasSize.width() - kCardWidth * scale) / 2.0,
                    freeHeight * 0.75, kCardWidth * scale, rootHeight);
  if (upgrade_.isEmpty()) {
    paintEmpty(painter, root, scale);
    drawSlotPolarity(painter, root, scale);
    return;
  }
  const FrameProfile profile = frameProfile(upgrade_);
  const qreal compactGap = kCardHeight - profile.contentHeight;
  const qreal contentHeight = cardHeight - compactGap;
  const QRectF content = mapped(root, scale, profile.contentX, 0.0,
                                profile.contentWidth, contentHeight);
  const QString effects = effectText(upgrade_);
  const ExpandedTextLayout expandedText =
      expandedTextLayout(font(), profile, upgradeName(upgrade_), effects);
  const qreal artworkHeight =
      mix(profile.artworkHeight, expandedText.artworkHeight, expansion);
  const QRectF artwork = mapped(root, scale, profile.artworkX, profile.artworkY,
                                profile.artworkWidth, artworkHeight);

  painter.save();
  painter.setClipRect(content);
  drawCovered(painter, content, staticPixmap(profile.background));
  painter.restore();

  painter.save();
  painter.setClipRect(artwork);
  const AssetRef asset = controller_ ? controller_->assetRef(id_) : AssetRef{};
  const QPixmap art =
      cachedThumbnail(painter, asset, QSize(192, 192), artwork.toAlignedRect());
  drawCovered(painter, artwork, art);
  QLinearGradient shade(artwork.topLeft(), artwork.bottomLeft());
  shade.setColorAt(0.0, QColor(0, 0, 0, qRound(mix(110, 55, expansion))));
  shade.setColorAt(0.45, QColor(0, 0, 0, qRound(mix(35, 15, expansion))));
  shade.setColorAt(1.0, QColor(0, 0, 0, qRound(mix(180, 105, expansion))));
  painter.fillRect(artwork, shade);
  painter.restore();

  QFont name = font();
  name.setPixelSize(std::max(11, qRound(17.0 * scale)));
  name.setWeight(QFont::Medium);
  painter.setFont(name);
  painter.setPen(QColor("#ffffff"));
  const qreal nameY = mix(24.0, expandedText.nameY, expansion);
  const qreal nameHeight = mix(48.0, expandedText.nameHeight, expansion);
  painter.drawText(
      mapped(root, scale, profile.nameX, nameY, profile.nameWidth, nameHeight),
      Qt::AlignCenter | Qt::TextWordWrap, upgradeName(upgrade_));

  if (!effects.isEmpty() && expansion > 0.0) {
    painter.save();
    painter.setOpacity(qBound(0.0, (expansion - 0.2) / 0.8, 1.0));
    QFont description = font();
    description.setPixelSize(std::max(8, qRound(13.0 * scale)));
    painter.setFont(description);
    painter.setPen(profile.textColor);
    const qreal effectsY =
        mix(24.0 + 48.0 + kExpandedTextGap, expandedText.effectsY, expansion);
    const qreal effectsHeight = mix(0.0, expandedText.effectsHeight, expansion);
    painter.drawText(mapped(root, scale, profile.nameX, effectsY,
                            profile.nameWidth, effectsHeight),
                     Qt::AlignHCenter | Qt::AlignTop | Qt::TextWordWrap,
                     effects);
    painter.restore();
  }

  drawSideLights(painter, root, scale, profile, expansion);
  drawDrain(painter, root, scale, profile, upgrade_, modPolarity_,
            polarityState_);
  drawFrameLayer(painter, root, scale, profile.top, profile.topWidth,
                 profile.topY, profile.topClipHeight);
  const qreal bottomY =
      mix(profile.bottomY, cardHeight - profile.bottomClipHeight, expansion);
  drawFrameLayer(painter, root, scale, profile.bottom, profile.bottomWidth,
                 bottomY, profile.bottomClipHeight);
  drawCompatibility(painter, root, scale, cardHeight, profile, upgrade_,
                    expansion);
  drawRankPips(painter, root, scale, profile.rankWidth, profile.rankBottom);
  drawSlotPolarity(painter, root, scale);
}

void ModCardWidget::paintEmpty(QPainter &painter, const QRectF &root,
                               qreal scale) const {
  painter.save();
  painter.setOpacity(slot_.value("unlocked").toBool(true) ? 0.8 : 0.3);
  const QPixmap shell = staticPixmap(":/resources/mod-slots/empty.png");
  if (!shell.isNull()) {
    painter.drawPixmap(root, shell,
                       QRectF(QPointF{}, shell.deviceIndependentSize()));
  }

  QString iconName;
  qreal iconHeight = 0.0;
  if (role_ == "aura") {
    iconName = "aura.png";
    iconHeight = 45.0;
  } else if (role_ == "exilus") {
    iconName = "exilus.png";
    iconHeight = 40.0;
  } else if (role_ == "stance") {
    iconName = "stance.png";
    iconHeight = 40.0;
  }
  const QPixmap icon = iconName.isEmpty()
                           ? QPixmap{}
                           : staticPixmap(":/resources/mod-slots/" + iconName);
  if (!icon.isNull()) {
    const QSizeF source = icon.deviceIndependentSize();
    const qreal height = iconHeight * scale;
    const qreal width = source.width() / source.height() * height;
    const QRectF destination(root.center().x() - width / 2.0,
                             root.center().y() - height / 2.0, width, height);
    painter.drawPixmap(destination, icon, QRectF(QPointF{}, source));
  }
  painter.restore();
}

void ModCardWidget::drawSlotPolarity(QPainter &painter, const QRectF &root,
                                     qreal scale) const {
  if (!showsExternalPolarity()) {
    return;
  }
  const QColor tint = polarityColor(polarityState_);
  const QPixmap glyph = modPolarityPixmap(slotPolarity_, tint);
  if (glyph.isNull()) {
    return;
  }
  const qreal edge = 14.0 * scale;
  const QRectF destination(root.right() - 22.0 * scale,
                           root.top() - 15.0 * scale, edge, edge);
  painter.drawPixmap(destination, glyph,
                     QRectF(QPointF{}, glyph.deviceIndependentSize()));
}

bool ModCardWidget::showsExternalPolarity() const {
  if (slotPolarity_.isEmpty() || slotPolarity_ == "none" ||
      slotPolarity_ == "unknown") {
    return false;
  }
  if (upgrade_.isEmpty()) {
    return true;
  }
  return polarityState_ == "mismatched" || slotPolarity_ == "omni" ||
         slotPolarity_ == "universal";
}

void ModCardWidget::drawRankPips(QPainter &painter, const QRectF &root,
                                 qreal scale, qreal width, qreal bottom) const {
  const int rank = upgrade_.value("rank").toInt(-1);
  const int maximum = upgrade_.value("max_rank").toInt(rank);
  if (rank < 0 || maximum <= 0) {
    return;
  }
  const int count = std::min(maximum, 10);
  const qreal trackWidth = width * scale;
  const qreal diameter = 5.0 * scale;
  const qreal pitch = 7.0 * scale;
  const qreal pipWidth = diameter + (count - 1) * pitch;
  const qreal y = root.bottom() - (bottom + 10.0) * scale;
  const qreal trackX = root.center().x() - trackWidth / 2.0;
  qreal x = root.center().x() - pipWidth / 2.0;

  if (rank >= maximum) {
    painter.setPen(
        QPen(QColor(166, 230, 255, 150), std::max(1.0, 0.8 * scale)));
    painter.drawLine(QPointF(trackX, y + diameter / 2.0),
                     QPointF(trackX + trackWidth, y + diameter / 2.0));
  }
  painter.setPen(Qt::NoPen);
  for (int index = 0; index < count; ++index) {
    painter.setBrush(index < rank ? QColor("#a6e6ff") : QColor("#525252"));
    painter.drawEllipse(QRectF(x, y, diameter, diameter));
    x += pitch;
  }
}

} // namespace wfgui
