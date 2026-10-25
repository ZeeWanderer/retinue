#include "player_item_grid_widget.h"

#include <QAbstractItemView>
#include <QContextMenuEvent>
#include <QFontMetrics>
#include <QHelpEvent>
#include <QMenu>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QResizeEvent>
#include <QScrollBar>
#include <QShowEvent>
#include <QStyleOptionViewItem>
#include <QStyledItemDelegate>
#include <QTimer>
#include <QVariantList>
#include <QVariantMap>

#include <algorithm>
#include <functional>
#include <utility>

#include "display_metrics.h"
#include "image_cache.h"
#include "inventory_card_layout.h"
#include "player_item_model.h"
#include "tooltip.h"
#include "widget_capture.h"

namespace {
constexpr int Gap = 8;
constexpr int MaxTrack = 620;
constexpr int FoundryMinTrack = 270;
constexpr int InventoryMinTrack = 420;
constexpr int MasteryMinTrack = 370;
constexpr int FoundryCardHeight = 142;
constexpr int FoundryBodyTop = 33;
constexpr int FoundryBodyHeight = 86;

struct FoundryLayout {
  QRect pending;
  QRect favorite;
  QRect favoriteIcon;
  QRect title;
  QRect mastered;
  QRect vaulted;
  QRect image;
  QRect mastery;
  QRect subsumed;
};

struct MasteryTitleLayout {
  QRect favorite;
  QRect title;
};

struct InventoryStatusLayout {
  QRect quantity;
  QRect vaulted;
  QRect mastered;
};

QFont cardTitleFont(QFont font, PlayerItemGridWidget::Kind kind, qreal scale) {
  const int pixels =
      kind == PlayerItemGridWidget::Kind::Foundry
          ? 20
          : (kind == PlayerItemGridWidget::Kind::Inventory ? 19 : 17);
  font.setPixelSize(wfgui::scaled(pixels, scale));
  font.setWeight(kind == PlayerItemGridWidget::Kind::Inventory ? QFont::Medium
                                                               : QFont::Normal);
  return font;
}

bool isFoundryWarframe(const QModelIndex &index) {
  const QString group = index.data(PlayerItemModel::GroupRole).toString();
  return group == "warframe" || group == "warframes";
}

FoundryLayout foundryLayout(const QRect &content, const QRect &card,
                            const QModelIndex &index, const QFont &font,
                            qreal scale) {
  const QString name = index.data(PlayerItemModel::NameRole).toString();
  const bool pending = index.data(PlayerItemModel::PendingRole).toBool();
  const bool mastered = index.data(PlayerItemModel::MasteredRole).toBool();
  const bool showVaulted = index.data(PlayerItemModel::IsPrimeRole).toBool();
  const bool warframe = isFoundryWarframe(index);
  const int statusSize = wfgui::scaled(24, scale);
  const int favoriteHitSize = wfgui::scaled(20, scale);
  const int favoriteIconSize = wfgui::scaled(16, scale);
  const int masteredSize = mastered ? wfgui::scaled(24, scale) : 0;
  const int gap = wfgui::scaled(5, scale);
  const int favoriteGap = wfgui::scaled(4, scale);
  const int masteredGap = wfgui::scaled(8, scale);
  const int top = content.top() + wfgui::scaled(1, scale);
  FoundryLayout layout;
  if (pending) {
    layout.pending = {content.left(), top, statusSize, statusSize};
  }
  if (showVaulted) {
    layout.vaulted = {content.right() - statusSize + 1, top, statusSize,
                      statusSize};
  }

  const int availableLeft =
      layout.pending.isNull() ? content.left() : layout.pending.right() + gap;
  const int availableRight =
      layout.vaulted.isNull() ? content.right() : layout.vaulted.left() - gap;
  const QRect available(availableLeft, content.top(),
                        std::max(0, availableRight - availableLeft + 1),
                        wfgui::scaled(27, scale));
  const int hitMargin = (favoriteHitSize - favoriteIconSize + 1) / 2;
  const QRect visualAvailable = available.adjusted(hitMargin, 0, -hitMargin, 0);
  const int fixed = favoriteIconSize + favoriteGap +
                    (mastered ? masteredSize + masteredGap : 0);
  const int maximumTitleWidth = std::max(0, visualAvailable.width() - fixed);
  const int naturalTitleWidth = QFontMetrics(font).horizontalAdvance(name);
  const int titleTextWidth = std::min(naturalTitleWidth, maximumTitleWidth);
  const int titleRectWidth =
      std::min(naturalTitleWidth + wfgui::scaled(6, scale), maximumTitleWidth);
  const int total = fixed + titleTextWidth;
  const int left = visualAvailable.center().x() - total / 2;
  layout.favoriteIcon = {left, content.top() + wfgui::scaled(5, scale),
                         favoriteIconSize, favoriteIconSize};
  layout.favorite = {layout.favoriteIcon.center().x() - favoriteHitSize / 2,
                     layout.favoriteIcon.center().y() - favoriteHitSize / 2,
                     favoriteHitSize, favoriteHitSize};
  layout.title = {layout.favoriteIcon.right() + favoriteGap + 1, content.top(),
                  titleRectWidth, wfgui::scaled(27, scale)};
  if (mastered) {
    layout.mastered = {layout.title.left() + titleTextWidth + masteredGap,
                       content.top() + wfgui::scaled(1, scale), masteredSize,
                       masteredSize};
  }

  const QRect body(content.left(),
                   content.top() + wfgui::scaled(FoundryBodyTop, scale),
                   content.width(), wfgui::scaled(FoundryBodyHeight, scale));
  const int imageSize =
      wfgui::scaled(warframe ? 105 : FoundryBodyHeight, scale);
  const int imageTop = warframe ? card.bottom() - imageSize + 1
                                : body.center().y() - imageSize / 2;
  layout.image = {content.left() + wfgui::scaled(3, scale), imageTop, imageSize,
                  imageSize};

  const int mastery =
      index.data(PlayerItemModel::MasteryRequirementRole).toInt();
  if (mastery > 1) {
    QFont badgeFont = font;
    badgeFont.setPixelSize(wfgui::scaled(12, scale));
    badgeFont.setWeight(QFont::Medium);
    const QString text = QString("MR %1").arg(mastery);
    const int height = wfgui::scaled(21, scale);
    const int width = QFontMetrics(badgeFont).horizontalAdvance(text) +
                      wfgui::scaled(12, scale);
    layout.mastery = {layout.image.left() + wfgui::scaled(2, scale),
                      layout.image.bottom() - height - wfgui::scaled(2, scale) +
                          1,
                      width, height};
  }
  if (warframe && index.data(PlayerItemModel::SubsumedRole).toBool()) {
    const int size = wfgui::scaled(25, scale);
    layout.subsumed = {
        layout.image.right() - size - wfgui::scaled(2, scale) + 1,
        layout.image.bottom() - size - wfgui::scaled(4, scale) + 1, size, size};
  }
  return layout;
}

QRect centeredSquare(const QRect &bounds, int size) {
  return {bounds.center().x() - size / 2, bounds.center().y() - size / 2, size,
          size};
}

void drawBottomContained(QPainter &painter, const QRectF &rect,
                         const QPixmap &image) {
  if (image.isNull()) {
    return;
  }
  QSizeF size = image.deviceIndependentSize();
  size.scale(rect.size(), Qt::KeepAspectRatio);
  const QRectF target(rect.center().x() - size.width() / 2.0,
                      rect.bottom() - size.height() + 1.0, size.width(),
                      size.height());
  painter.drawPixmap(target, image, QRectF(image.rect()));
}

void drawSourceContained(QPainter &painter, const QRectF &rect,
                         const QPixmap &image, const QSize &sourceSize,
                         const QRect &sourceContent) {
  if (image.isNull()) {
    return;
  }
  const QRectF imageRect(image.rect());
  const QRectF source(
      imageRect.width() * sourceContent.x() / sourceSize.width(),
      imageRect.height() * sourceContent.y() / sourceSize.height(),
      imageRect.width() * sourceContent.width() / sourceSize.width(),
      imageRect.height() * sourceContent.height() / sourceSize.height());
  QSizeF size(sourceContent.size());
  size.scale(rect.size(), Qt::KeepAspectRatio);
  const QRectF target(rect.center().x() - size.width() / 2.0,
                      rect.center().y() - size.height() / 2.0, size.width(),
                      size.height());
  painter.drawPixmap(target, image, source);
}

QList<QRect> componentRects(const QRect &content, int componentCount,
                            PlayerItemGridWidget::Kind kind, qreal scale) {
  const int count = std::min(6, componentCount);
  const int size = wfgui::scaled(
      kind == PlayerItemGridWidget::Kind::Mastery
          ? 30
          : (kind == PlayerItemGridWidget::Kind::Foundry ? 39 : 34),
      scale);
  const int gap =
      wfgui::scaled(kind == PlayerItemGridWidget::Kind::Foundry ? 8 : 5, scale);
  if (kind == PlayerItemGridWidget::Kind::Foundry) {
    const int columns = 3;
    const int width = columns * size + (columns - 1) * gap;
    const int areaStart = content.right() - width + 1;
    const int rows = (count + columns - 1) / columns;
    const int totalHeight = rows * size + std::max(0, rows - 1) * gap;
    const QRect body(content.left(),
                     content.top() + wfgui::scaled(FoundryBodyTop, scale),
                     content.width(), wfgui::scaled(FoundryBodyHeight, scale));
    const int top = body.center().y() - totalHeight / 2;
    QList<QRect> result;
    result.reserve(count);
    for (int row = 0, index = 0; index < count; ++row) {
      const int rowCount = std::min(columns, count - index);
      const int rowWidth = rowCount * size + (rowCount - 1) * gap;
      const int rowStart = areaStart + (width - rowWidth) / 2;
      for (int column = 0; column < rowCount; ++column, ++index) {
        result.append({rowStart + column * (size + gap),
                       top + row * (size + gap), size, size});
      }
    }
    return result;
  }
  if (kind == PlayerItemGridWidget::Kind::Mastery) {
    constexpr int columns = 3;
    const int areaWidth = wfgui::scaled(109, scale);
    const int areaLeft = content.right() - areaWidth + 1;
    const int rows = (count + columns - 1) / columns;
    const int totalHeight = rows * size + std::max(0, rows - 1) * gap;
    const int top = content.center().y() - totalHeight / 2;
    QList<QRect> result;
    result.reserve(count);
    for (int row = 0, index = 0; index < count; ++row) {
      const int rowCount = std::min(columns, count - index);
      const int rowWidth = rowCount * size + (rowCount - 1) * gap;
      const int rowStart = areaLeft + (areaWidth - rowWidth) / 2;
      for (int column = 0; column < rowCount; ++column, ++index) {
        result.append({rowStart + column * (size + gap),
                       top + row * (size + gap), size, size});
      }
    }
    return result;
  }
  return {};
}

QRect masteryMiddleRect(const QRect &content, qreal scale) {
  const int imageWidth = wfgui::scaled(72, scale);
  const int rightWidth = wfgui::scaled(109, scale);
  return {content.left() + imageWidth + wfgui::scaled(4, scale), content.top(),
          content.width() - imageWidth - rightWidth - wfgui::scaled(8, scale),
          content.height()};
}

MasteryTitleLayout masteryTitleLayout(const QRect &content, const QString &name,
                                      const QFont &font, qreal scale) {
  const QRect middle = masteryMiddleRect(content, scale);
  const int favoriteSize = wfgui::scaled(20, scale);
  const int gap = wfgui::scaled(5, scale);
  const int titleWidth = std::min(
      QFontMetrics(font).horizontalAdvance(name) + wfgui::scaled(4, scale),
      std::max(0, middle.width() - favoriteSize - gap));
  const int total = favoriteSize + gap + titleWidth;
  const int left = middle.center().x() - total / 2;
  return {{left, content.top() + wfgui::scaled(3, scale), favoriteSize,
           favoriteSize},
          {left + favoriteSize + gap, content.top(), titleWidth,
           wfgui::scaled(27, scale)}};
}

QString probabilityText(double probability) {
  const double percent = probability * 100.0;
  QString text = QString::number(percent, 'f', percent < 10.0 ? 2 : 1);
  while (text.contains('.') && text.endsWith('0')) {
    text.chop(1);
  }
  if (text.endsWith('.')) {
    text.chop(1);
  }
  return text + '%';
}

QRect contentRect(const QRect &itemRect, PlayerItemGridWidget::Kind kind,
                  qreal scale) {
  const int inset = wfgui::scaled(4, scale);
  const QRect card = itemRect.adjusted(inset, inset, -inset, -inset);
  const int padding = wfgui::scaled(
      kind == PlayerItemGridWidget::Kind::Inventory ? 6 : 8, scale);
  return card.adjusted(padding, padding, -padding, -padding);
}

InventoryStatusLayout inventoryStatusLayout(const QRect &area,
                                            const QString &quantity,
                                            bool showVaulted, bool vaulted,
                                            bool mastered, bool compact,
                                            const QFont &font, qreal scale) {
  const QFontMetrics metrics(font);
  const int icon = wfgui::scaled(compact ? 18 : 19, scale);
  const int gap = wfgui::scaled(4, scale);
  const QString vaultText = compact ? QString()
                                    : (vaulted ? QStringLiteral("Vaulted")
                                               : QStringLiteral("Unvaulted"));
  const int quantityWidth = metrics.horizontalAdvance(quantity);
  const int vaultedWidth =
      showVaulted ? icon + (vaultText.isEmpty()
                                ? 0
                                : gap + metrics.horizontalAdvance(vaultText))
                  : 0;
  const int masteredWidth = mastered ? wfgui::scaled(20, scale) : 0;
  const int count =
      1 + static_cast<int>(showVaulted) + static_cast<int>(mastered);
  const int used = quantityWidth + vaultedWidth + masteredWidth;
  const int spacing =
      std::max(wfgui::scaled(2, scale), (area.width() - used) / (count + 1));
  int x = area.left() + spacing;
  InventoryStatusLayout result;
  result.quantity = {x, area.top(), quantityWidth, area.height()};
  x += quantityWidth + spacing;
  if (showVaulted) {
    result.vaulted = {x, area.center().y() - icon / 2, vaultedWidth, icon};
    x += vaultedWidth + spacing;
  }
  if (mastered) {
    result.mastered = {x, area.center().y() - masteredWidth / 2, masteredWidth,
                       masteredWidth};
  }
  return result;
}

QRect ownedRelicMarkerRect(const QRect &component, qreal scale) {
  const int size = wfgui::scaled(19, scale);
  return {component.left() - wfgui::scaled(2, scale),
          component.bottom() - size + wfgui::scaled(3, scale), size, size};
}

QString componentMarketName(const QVariantMap &component) {
  const QString marketName = component.value("market_name").toString();
  return marketName.isEmpty() ? component.value("name").toString() : marketName;
}

QString counterpartyOrderSide(int owned, int required) {
  // Missing items come from WTS sellers; owned items go to WTB buyers.
  return owned >= qMax(1, required) ? "buy" : "sell";
}

QString relicSearchTerm(const QModelIndex &index) {
  const QString group = index.data(PlayerItemModel::GroupRole).toString();
  if (group == "relics" ||
      index.data(PlayerItemModel::FromRelicsRole).toBool()) {
    return index.data(PlayerItemModel::NameRole).toString();
  }
  if (!index.data(PlayerItemModel::IsPrimeRole).toBool()) {
    return {};
  }
  if (group == "parts") {
    return index.data(PlayerItemModel::MarketNameRole).toString();
  }
  if (group == "sets") {
    QString name = index.data(PlayerItemModel::NameRole).toString();
    if (name.endsWith(" Set", Qt::CaseInsensitive)) {
      name.chop(4);
    }
    return name;
  }
  return index.data(PlayerItemModel::NameRole).toString();
}

struct ContextTarget {
  QModelIndex item;
  QVariantMap component;
};

ContextTarget contextTargetAt(const PlayerItemGridWidget *view,
                              PlayerItemGridWidget::Kind kind,
                              const QPoint &position) {
  const QModelIndex index = view->indexAt(position);
  if (!index.isValid()) {
    return {};
  }

  const qreal scale = wfgui::displayScale(view);
  const QRect content = contentRect(view->visualRect(index), kind, scale);
  QVariantList components =
      index.data(PlayerItemModel::ComponentsRole).toList();
  QList<QRect> areas;
  if (kind == PlayerItemGridWidget::Kind::Inventory) {
    const bool isSet =
        index.data(PlayerItemModel::GroupRole).toString() == "sets";
    areas = wfgui::InventoryCardLayout::calculate(
                content, isSet, static_cast<int>(components.size()), scale)
                .components;
  } else {
    if (kind == PlayerItemGridWidget::Kind::Mastery &&
        index.data(PlayerItemModel::OwnedRole).toBool()) {
      components.clear();
    }
    areas = componentRects(content, static_cast<int>(components.size()), kind,
                           scale);
  }
  for (int component = 0;
       component < std::min(static_cast<int>(areas.size()),
                            static_cast<int>(components.size()));
       ++component) {
    if (areas.at(component).contains(position)) {
      return {index, components.at(component).toMap()};
    }
  }
  return {index, {}};
}

class PlayerItemDelegate final : public QStyledItemDelegate {
public:
  explicit PlayerItemDelegate(
      PlayerItemGridWidget::Kind kind,
      std::function<void(const QString &, const QString &)> marketRequest,
      std::function<void(const QString &)> relicRequest, QObject *parent)
      : QStyledItemDelegate(parent), kind_(kind),
        marketRequest_(std::move(marketRequest)),
        relicRequest_(std::move(relicRequest)) {}

  QSize sizeHint(const QStyleOptionViewItem &,
                 const QModelIndex &) const override {
    const auto *view = qobject_cast<const QListView *>(parent());
    return view ? view->gridSize() : QSize(400, 150);
  }

  void paint(QPainter *painter, const QStyleOptionViewItem &option,
             const QModelIndex &index) const override {
    painter->save();
    painter->setRenderHint(QPainter::Antialiasing);
    painter->setRenderHint(QPainter::SmoothPixmapTransform);
    const qreal scale =
        wfgui::displayScale(qobject_cast<const QWidget *>(parent()));
    const int inset = wfgui::scaled(4, scale);
    const QRect card = option.rect.adjusted(inset, inset, -inset, -inset);
    const bool foundryOwned = kind_ == PlayerItemGridWidget::Kind::Foundry &&
                              index.data(PlayerItemModel::OwnedRole).toBool();
    painter->setPen(Qt::NoPen);
    painter->setBrush(
        foundryOwned
            ? QColor(option.state & QStyle::State_MouseOver ? "#24523f"
                                                            : "#204638")
            : QColor(option.state & QStyle::State_MouseOver ? "#272f47"
                                                            : "#20283e"));
    const int radius = wfgui::scaled(
        kind_ == PlayerItemGridWidget::Kind::Inventory ? 20 : 12, scale);
    painter->drawRoundedRect(card, radius, radius);

    const QRect content = contentRect(option.rect, kind_, scale);
    if (kind_ == PlayerItemGridWidget::Kind::Foundry) {
      painter->save();
      QPainterPath clip;
      clip.addRoundedRect(QRectF(card), radius, radius);
      painter->setClipPath(clip);
      paintFoundry(*painter, content, card, index, option.font, scale,
                   option.rect);
      painter->restore();
      if (foundryOwned) {
        painter->setPen(QPen(QColor("#248444"), wfgui::scaled(2, scale)));
        painter->setBrush(Qt::NoBrush);
        painter->drawRoundedRect(card, radius, radius);
      }
      painter->restore();
      return;
    }
    if (kind_ == PlayerItemGridWidget::Kind::Mastery) {
      paintMastery(*painter, content, index, option.font, scale, option.rect);
      painter->restore();
      return;
    }
    paintInventory(*painter, content, index, option.font, scale, option.rect);
    painter->restore();
  }

  bool helpEvent(QHelpEvent *event, QAbstractItemView *view,
                 const QStyleOptionViewItem &option,
                 const QModelIndex &index) override {
    if (event->type() == QEvent::ToolTip) {
      const qreal scale = wfgui::displayScale(view);
      const QRect content = contentRect(option.rect, kind_, scale);
      if (kind_ == PlayerItemGridWidget::Kind::Foundry) {
        const QFont titleFont = cardTitleFont(
            option.font, PlayerItemGridWidget::Kind::Foundry, scale);
        const int inset = wfgui::scaled(4, scale);
        const QRect card = option.rect.adjusted(inset, inset, -inset, -inset);
        const FoundryLayout layout =
            foundryLayout(content, card, index, titleFont, scale);
        if (layout.favorite.contains(event->pos())) {
          wfgui::showTooltip(view->viewport(), event->pos(), "Favorite",
                             layout.favorite);
          return true;
        }
        if (layout.pending.contains(event->pos())) {
          wfgui::showTooltip(view->viewport(), event->pos(), "In Foundry",
                             layout.pending);
          return true;
        }
        if (layout.mastered.contains(event->pos())) {
          wfgui::showTooltip(view->viewport(), event->pos(), "Mastered",
                             layout.mastered);
          return true;
        }
        if (layout.vaulted.contains(event->pos())) {
          wfgui::showTooltip(view->viewport(), event->pos(),
                             index.data(PlayerItemModel::VaultedRole).toBool()
                                 ? "Vaulted"
                                 : "Unvaulted",
                             layout.vaulted);
          return true;
        }
        if (layout.mastery.contains(event->pos())) {
          wfgui::showTooltip(
              view->viewport(), event->pos(),
              QString("Mastery Rank %1")
                  .arg(index.data(PlayerItemModel::MasteryRequirementRole)
                           .toInt()),
              layout.mastery);
          return true;
        }
        if (layout.subsumed.contains(event->pos())) {
          wfgui::showTooltip(view->viewport(), event->pos(), "Subsumed",
                             layout.subsumed);
          return true;
        }
      } else if (kind_ == PlayerItemGridWidget::Kind::Mastery) {
        const QFont titleFont = cardTitleFont(
            option.font, PlayerItemGridWidget::Kind::Mastery, scale);
        const MasteryTitleLayout title = masteryTitleLayout(
            content, index.data(PlayerItemModel::NameRole).toString(),
            titleFont, scale);
        if (title.favorite.contains(event->pos())) {
          wfgui::showTooltip(view->viewport(), event->pos(), "Favorite",
                             title.favorite);
          return true;
        }
      }
      QVariantList components =
          index.data(PlayerItemModel::ComponentsRole).toList();
      if (kind_ == PlayerItemGridWidget::Kind::Mastery &&
          index.data(PlayerItemModel::OwnedRole).toBool()) {
        components.clear();
      }
      QList<QRect> rects;
      if (kind_ == PlayerItemGridWidget::Kind::Inventory) {
        const bool isSet =
            index.data(PlayerItemModel::GroupRole).toString() == "sets";
        const wfgui::InventoryCardLayout layout =
            wfgui::InventoryCardLayout::calculate(
                content, isSet, static_cast<int>(components.size()), scale);
        QFont statusFont = option.font;
        statusFont.setPixelSize(wfgui::scaled(14, scale));
        statusFont.setWeight(QFont::Normal);
        const bool showVaulted =
            index.data(PlayerItemModel::IsPrimeRole).toBool() ||
            index.data(PlayerItemModel::GroupRole).toString() == "relics";
        const InventoryStatusLayout status = inventoryStatusLayout(
            layout.status,
            QString("x%1").arg(
                index.data(PlayerItemModel::QuantityRole).toInt()),
            showVaulted, index.data(PlayerItemModel::VaultedRole).toBool(),
            index.data(PlayerItemModel::MasteredRole).toBool(), isSet,
            statusFont, scale);
        if (status.mastered.contains(event->pos())) {
          wfgui::showTooltip(view->viewport(), event->pos(),
                             "Item owned/mastered", status.mastered);
          return true;
        }
        if (index.data(PlayerItemModel::TradableRole).toBool() &&
            !index.data(PlayerItemModel::SellableRole).toBool() &&
            layout.sell.contains(event->pos())) {
          wfgui::showTooltip(
              view->viewport(), event->pos(),
              isSet ? "No complete set owned" : "No copies owned", layout.sell);
          return true;
        }
        rects = layout.components;
      } else {
        rects = componentRects(content, static_cast<int>(components.size()),
                               kind_, scale);
      }
      for (int componentIndex = 0; componentIndex < rects.size();
           ++componentIndex) {
        if (!rects.at(componentIndex).contains(event->pos())) {
          continue;
        }
        const QVariantMap component = components.at(componentIndex).toMap();
        QString text = component.value("name").toString();
        if (kind_ == PlayerItemGridWidget::Kind::Foundry &&
            component.value("owned_relic").toBool()) {
          const QRect marker =
              ownedRelicMarkerRect(rects.at(componentIndex), scale);
          if (marker.contains(event->pos())) {
            wfgui::showTooltip(view->viewport(), event->pos(),
                               QString("Show relics containing %1")
                                   .arg(componentMarketName(component)),
                               marker);
            return true;
          }
        }
        const int required = component.value("required").toInt();
        if (required > 0) {
          text += QString("\nOwned: %1/%2")
                      .arg(component.value("owned").toInt())
                      .arg(required);
        }
        const int blueprints = component.value("blueprint_owned").toInt();
        if (blueprints > 0) {
          text += QString("\nBlueprints: %1").arg(blueprints);
        }
        wfgui::showTooltip(view->viewport(), event->pos(), text,
                           rects.at(componentIndex));
        return true;
      }
      const QString text = index.data(Qt::ToolTipRole).toString();
      if (!text.isEmpty()) {
        wfgui::showTooltip(view->viewport(), event->pos(), text, option.rect);
        return true;
      }
    }
    return QStyledItemDelegate::helpEvent(event, view, option, index);
  }

  bool editorEvent(QEvent *event, QAbstractItemModel *model,
                   const QStyleOptionViewItem &option,
                   const QModelIndex &index) override {
    if (event->type() == QEvent::MouseButtonRelease) {
      const auto *mouse = static_cast<QMouseEvent *>(event);
      if (mouse->button() == Qt::LeftButton) {
        const qreal scale =
            wfgui::displayScale(qobject_cast<const QWidget *>(parent()));
        const QRect content = contentRect(option.rect, kind_, scale);
        const QPoint position = mouse->position().toPoint();
        if (kind_ == PlayerItemGridWidget::Kind::Foundry ||
            kind_ == PlayerItemGridWidget::Kind::Mastery) {
          const QFont titleFont = cardTitleFont(option.font, kind_, scale);
          const QRect favorite =
              kind_ == PlayerItemGridWidget::Kind::Foundry
                  ? foundryLayout(
                        content,
                        option.rect.adjusted(
                            wfgui::scaled(4, scale), wfgui::scaled(4, scale),
                            -wfgui::scaled(4, scale), -wfgui::scaled(4, scale)),
                        index, titleFont, scale)
                        .favorite
                  : masteryTitleLayout(
                        content,
                        index.data(PlayerItemModel::NameRole).toString(),
                        titleFont, scale)
                        .favorite;
          if (favorite.contains(position)) {
            return model->setData(
                index, !index.data(PlayerItemModel::FavoriteRole).toBool(),
                PlayerItemModel::FavoriteRole);
          }
        }

        QVariantList components =
            index.data(PlayerItemModel::ComponentsRole).toList();
        QList<QRect> componentAreas;
        if (kind_ == PlayerItemGridWidget::Kind::Inventory) {
          const bool isSet =
              index.data(PlayerItemModel::GroupRole).toString() == "sets";
          const wfgui::InventoryCardLayout layout =
              wfgui::InventoryCardLayout::calculate(
                  content, isSet, static_cast<int>(components.size()), scale);
          if (index.data(PlayerItemModel::TradableRole).toBool()) {
            const QString item =
                index.data(PlayerItemModel::MarketNameRole).toString();
            if (!item.isEmpty() && layout.sell.contains(position)) {
              marketRequest_(item, "sell");
              return true;
            }
            if (!item.isEmpty() && !isSet && layout.buy.contains(position)) {
              marketRequest_(item, "buy");
              return true;
            }
          }
          componentAreas = layout.components;
        } else {
          if (kind_ == PlayerItemGridWidget::Kind::Mastery &&
              index.data(PlayerItemModel::OwnedRole).toBool()) {
            components.clear();
          }
          componentAreas = componentRects(
              content, static_cast<int>(components.size()), kind_, scale);
        }
        for (int component = 0; component < componentAreas.size();
             ++component) {
          if (!componentAreas.at(component).contains(position)) {
            continue;
          }
          const QVariantMap data = components.at(component).toMap();
          const QString item = componentMarketName(data);
          if (!item.isEmpty()) {
            const int owned = data.value("owned").toInt();
            const int required = data.value("required").toInt();
            if (kind_ == PlayerItemGridWidget::Kind::Foundry &&
                data.value("owned_relic").toBool() &&
                ownedRelicMarkerRect(componentAreas.at(component), scale)
                    .contains(position)) {
              relicRequest_(item);
              return true;
            }
            marketRequest_(item, counterpartyOrderSide(owned, required));
            return true;
          }
        }
      }
    }
    return QStyledItemDelegate::editorEvent(event, model, option, index);
  }

private:
  static void paintFoundry(QPainter &painter, const QRect &content,
                           const QRect &card, const QModelIndex &index,
                           const QFont &baseFont, qreal scale,
                           const QRect &dirtyRegion) {
    const QFont titleFont =
        cardTitleFont(baseFont, PlayerItemGridWidget::Kind::Foundry, scale);
    painter.setFont(titleFont);
    painter.setPen(QColor("#ffffff"));
    const QString name = index.data(PlayerItemModel::NameRole).toString();
    const FoundryLayout layout =
        foundryLayout(content, card, index, titleFont, scale);

    if (!layout.pending.isNull()) {
      const QRect badge =
          centeredSquare(layout.pending, wfgui::scaled(18, scale));
      painter.setPen(Qt::NoPen);
      painter.setBrush(QColor("#365ca0"));
      painter.drawEllipse(badge);
      const QRect icon = centeredSquare(badge, wfgui::scaled(11, scale));
      wfgui::drawContained(painter, icon,
                           wfgui::cachedThumbnail(painter,
                                                  ":/resources/ui/pending.png",
                                                  icon.size()));
    }
    wfgui::drawContained(painter, layout.favoriteIcon,
                         wfgui::cachedThumbnail(
                             painter,
                             index.data(PlayerItemModel::FavoriteRole).toBool()
                                 ? ":/resources/ui/favorite_active.png"
                                 : ":/resources/ui/favorite.png",
                             layout.favoriteIcon.size()));
    painter.setPen(QColor("#ffffff"));
    painter.drawText(layout.title, Qt::AlignLeft | Qt::AlignVCenter,
                     QFontMetrics(titleFont).elidedText(name, Qt::ElideRight,
                                                        layout.title.width()));
    if (!layout.mastered.isNull()) {
      wfgui::drawContained(painter, layout.mastered,
                           wfgui::cachedThumbnail(painter,
                                                  ":/resources/ui/mastered.png",
                                                  layout.mastered.size()));
    }
    if (!layout.vaulted.isNull()) {
      const bool vaulted = index.data(PlayerItemModel::VaultedRole).toBool();
      const QString path =
          vaulted ? ":/assets/vaulted.png" : ":/assets/unvaulted.png";
      // Upstream glyphs carry large transparent margins.
      const QSize sourceSize = vaulted ? QSize(182, 173) : QSize(183, 187);
      const QRect sourceContent =
          vaulted ? QRect(38, 51, 106, 80) : QRect(29, 29, 122, 130);
      const QRect target = centeredSquare(
          layout.vaulted, wfgui::scaled(vaulted ? 18 : 17, scale));
      const int decodeSize = wfgui::scaled(48, scale);
      drawSourceContained(
          painter, target,
          wfgui::cachedThumbnail(painter, path, QSize(decodeSize, decodeSize)),
          sourceSize, sourceContent);
    }

    const QPixmap image = wfgui::cachedThumbnail(
        painter,
        index.data(PlayerItemModel::AssetRefRole).value<wfgui::AssetRef>(),
        layout.image.size(), dirtyRegion);
    if (isFoundryWarframe(index)) {
      drawBottomContained(painter, layout.image, image);
    } else {
      wfgui::drawContained(painter, layout.image, image);
    }
    paintComponents(painter, content,
                    index.data(PlayerItemModel::ComponentsRole).toList(),
                    PlayerItemGridWidget::Kind::Foundry, scale, dirtyRegion);

    if (!layout.mastery.isNull()) {
      painter.setPen(QPen(QColor("#566078"), wfgui::scaled(1, scale)));
      painter.setBrush(QColor(15, 20, 32, 0xd9));
      const int radius = layout.mastery.height() / 2;
      painter.drawRoundedRect(layout.mastery, radius, radius);
      QFont badgeFont = baseFont;
      badgeFont.setPixelSize(wfgui::scaled(12, scale));
      badgeFont.setWeight(QFont::Medium);
      painter.setFont(badgeFont);
      painter.setPen(QColor("#ffffff"));
      painter.drawText(
          layout.mastery, Qt::AlignCenter,
          QString("MR %1").arg(
              index.data(PlayerItemModel::MasteryRequirementRole).toInt()));
    }
    if (!layout.subsumed.isNull()) {
      painter.setPen(QPen(QColor("#b86767"), wfgui::scaled(1, scale)));
      painter.setBrush(QColor("#7e3a3a"));
      painter.drawEllipse(layout.subsumed);
      const QRect icon = layout.subsumed.adjusted(
          wfgui::scaled(4, scale), wfgui::scaled(4, scale),
          -wfgui::scaled(4, scale), -wfgui::scaled(4, scale));
      wfgui::drawContained(painter, icon,
                           wfgui::cachedThumbnail(painter,
                                                  ":/resources/ui/helminth.png",
                                                  icon.size()));
    }
  }

  static void paintInventory(QPainter &painter, const QRect &content,
                             const QModelIndex &index, const QFont &baseFont,
                             qreal scale, const QRect &dirtyRegion) {
    const QVariantList components =
        index.data(PlayerItemModel::ComponentsRole).toList();
    const bool isSet =
        index.data(PlayerItemModel::GroupRole).toString() == "sets";
    const wfgui::InventoryCardLayout layout =
        wfgui::InventoryCardLayout::calculate(
            content, isSet, static_cast<int>(components.size()), scale);

    const QRect imageRect = layout.image.adjusted(
        wfgui::scaled(2, scale), wfgui::scaled(2, scale),
        -wfgui::scaled(2, scale), -wfgui::scaled(2, scale));
    wfgui::drawContained(
        painter, imageRect,
        wfgui::cachedThumbnail(
            painter,
            index.data(PlayerItemModel::AssetRefRole).value<wfgui::AssetRef>(),
            imageRect.size(), dirtyRegion));

    const QFont titleFont =
        cardTitleFont(baseFont, PlayerItemGridWidget::Kind::Inventory, scale);
    painter.setFont(titleFont);
    painter.setPen(QColor("#ffffff"));
    const QString name = index.data(PlayerItemModel::NameRole).toString();
    painter.drawText(layout.title, Qt::AlignCenter,
                     QFontMetrics(titleFont).elidedText(name, Qt::ElideRight,
                                                        layout.title.width()));

    QFont detailFont = baseFont;
    detailFont.setPixelSize(wfgui::scaled(14, scale));
    detailFont.setWeight(QFont::Normal);
    const QString quantity =
        QString("x%1").arg(index.data(PlayerItemModel::QuantityRole).toInt());
    const QVariant vaultedValue = index.data(PlayerItemModel::VaultedRole);
    const bool showVaulted =
        vaultedValue.isValid() &&
        (index.data(PlayerItemModel::IsPrimeRole).toBool() ||
         index.data(PlayerItemModel::GroupRole).toString() == "relics");
    const bool vaulted = vaultedValue.toBool();
    const bool mastered = index.data(PlayerItemModel::MasteredRole).toBool();
    const InventoryStatusLayout status =
        inventoryStatusLayout(layout.status, quantity, showVaulted, vaulted,
                              mastered, isSet, detailFont, scale);
    paintInventoryStatus(painter, status, quantity, showVaulted, vaulted,
                         mastered, isSet, detailFont, scale);

    const int ducats = index.data(PlayerItemModel::DucatsRole).toInt();
    if (ducats > 0) {
      QFont ducatFont = detailFont;
      ducatFont.setPixelSize(wfgui::scaled(16, scale));
      ducatFont.setWeight(QFont::DemiBold);
      painter.setFont(ducatFont);
      const QString text = QString::number(ducats);
      const int iconSize = wfgui::scaled(22, scale);
      const int gap = wfgui::scaled(4, scale);
      const int textWidth = QFontMetrics(ducatFont).horizontalAdvance(text);
      const int width = textWidth + gap + iconSize;
      const int left = layout.ducats.center().x() - width / 2;
      painter.setPen(QColor("#ffffff"));
      painter.drawText(
          QRect(left, layout.ducats.top(), textWidth, layout.ducats.height()),
          Qt::AlignCenter, text);
      const QRect icon(left + textWidth + gap,
                       layout.ducats.center().y() - iconSize / 2, iconSize,
                       iconSize);
      wfgui::drawContained(
          painter, icon,
          wfgui::cachedThumbnail(painter, ":/assets/ducats.png", icon.size()));
    }

    if (isSet) {
      paintComponentRects(painter, components, layout.components,
                          PlayerItemGridWidget::Kind::Inventory, scale,
                          dirtyRegion);
    }
    if (!index.data(PlayerItemModel::TradableRole).toBool()) {
      return;
    }
    painter.setFont(detailFont);
    const QString state =
        index.data(PlayerItemModel::PriceStateRole).toString();
    const auto priceText = [&index, &state](int role) {
      return state == "loading"
                 ? QString("...")
                 : (state == "ready" && index.data(role).isValid()
                        ? index.data(role).toString()
                        : QString("--"));
    };
    paintQuotePill(painter, layout.sell, isSet ? "SELL" : "WTS",
                   priceText(PlayerItemModel::PlatinumRole), QColor("#6f2e43"),
                   scale, !index.data(PlayerItemModel::SellableRole).toBool());
    if (!isSet) {
      paintQuotePill(painter, layout.buy, "WTB",
                     priceText(PlayerItemModel::BuyPlatinumRole),
                     QColor("#1e5b50"), scale);
    }
  }

  static void paintInventoryStatus(QPainter &painter,
                                   const InventoryStatusLayout &layout,
                                   const QString &quantity, bool showVaulted,
                                   bool vaulted, bool mastered, bool compact,
                                   const QFont &font, qreal scale) {
    painter.setFont(font);
    painter.setPen(QColor("#ffffff"));
    painter.drawText(layout.quantity, Qt::AlignCenter, quantity);
    if (showVaulted) {
      const int iconSize = layout.vaulted.height();
      const QRect icon(layout.vaulted.left(), layout.vaulted.top(), iconSize,
                       iconSize);
      wfgui::drawContained(
          painter, icon,
          wfgui::cachedThumbnail(painter, ":/assets/vaulted.png", icon.size()));
      if (!compact) {
        painter.drawText(layout.vaulted.adjusted(
                             iconSize + wfgui::scaled(4, scale), 0, 0, 0),
                         Qt::AlignLeft | Qt::AlignVCenter,
                         vaulted ? "Vaulted" : "Unvaulted");
      }
    }
    if (mastered) {
      paintMasteredCheck(painter, layout.mastered, scale);
    }
  }

  static void paintMasteredCheck(QPainter &painter, const QRect &mark,
                                 qreal scale) {
    painter.setPen(Qt::NoPen);
    painter.setBrush(QColor("#3bb54a"));
    painter.drawEllipse(mark);
    QPen check(QColor("#d4e1f4"), std::max(1.5, 1.7 * scale));
    check.setCapStyle(Qt::RoundCap);
    check.setJoinStyle(Qt::RoundJoin);
    painter.setPen(check);
    QPainterPath path;
    path.moveTo(mark.left() + mark.width() * 0.24,
                mark.top() + mark.height() * 0.51);
    path.lineTo(mark.left() + mark.width() * 0.43,
                mark.top() + mark.height() * 0.69);
    path.lineTo(mark.left() + mark.width() * 0.76,
                mark.top() + mark.height() * 0.31);
    painter.drawPath(path);
  }

  static void paintQuotePill(QPainter &painter, const QRect &rect,
                             const QString &label, const QString &price,
                             const QColor &color, qreal scale,
                             bool warning = false) {
    painter.setPen(Qt::NoPen);
    painter.setBrush(warning ? QColor("#390d0d") : color);
    painter.drawRoundedRect(rect, rect.height() / 2, rect.height() / 2);
    painter.setPen(warning ? QColor("#d2d2d2") : QColor("#ffffff"));
    painter.drawText(rect.adjusted(wfgui::scaled(10, scale), 0, 0, 0),
                     Qt::AlignLeft | Qt::AlignVCenter, label);
    const int iconSize = wfgui::scaled(16, scale);
    const int priceWidth =
        QFontMetrics(painter.font()).horizontalAdvance(price);
    const int groupWidth = priceWidth + wfgui::scaled(3, scale) + iconSize;
    const int left = rect.right() - wfgui::scaled(10, scale) - groupWidth + 1;
    painter.drawText(QRect(left, rect.top(), priceWidth, rect.height()),
                     Qt::AlignCenter, price);
    const QRect icon(left + priceWidth + wfgui::scaled(3, scale),
                     rect.center().y() - iconSize / 2, iconSize, iconSize);
    wfgui::drawContained(
        painter, icon,
        wfgui::cachedThumbnail(painter, ":/assets/platinum.png", icon.size()));
  }

  static void paintMastery(QPainter &painter, const QRect &content,
                           const QModelIndex &index, const QFont &baseFont,
                           qreal scale, const QRect &dirtyRegion) {
    const bool owned = index.data(PlayerItemModel::OwnedRole).toBool();
    const QVariantList components =
        index.data(PlayerItemModel::ComponentsRole).toList();

    const int imageSize = wfgui::scaled(72, scale);
    const QRect imageRect(content.left(), content.center().y() - imageSize / 2,
                          imageSize, imageSize);
    wfgui::drawContained(
        painter, imageRect,
        wfgui::cachedThumbnail(
            painter,
            index.data(PlayerItemModel::AssetRefRole).value<wfgui::AssetRef>(),
            imageRect.size(), dirtyRegion));

    const int xpHeight = wfgui::scaled(21, scale);
    const QRect xpBadge(imageRect.left(), imageRect.bottom() - xpHeight + 1,
                        imageRect.width(), xpHeight);
    painter.setPen(Qt::NoPen);
    painter.setBrush(QColor(0, 0, 0, 0x8a));
    painter.drawRoundedRect(xpBadge, wfgui::scaled(5, scale),
                            wfgui::scaled(5, scale));
    painter.setPen(QColor("#ffffff"));
    painter.drawText(xpBadge, Qt::AlignCenter,
                     QString("+%1 XP").arg(
                         index.data(PlayerItemModel::PotentialXpRole).toInt()));

    const QFont titleFont =
        cardTitleFont(baseFont, PlayerItemGridWidget::Kind::Mastery, scale);
    const QString name = index.data(PlayerItemModel::NameRole).toString();
    const MasteryTitleLayout title =
        masteryTitleLayout(content, name, titleFont, scale);
    wfgui::drawContained(painter, title.favorite,
                         wfgui::cachedThumbnail(
                             painter,
                             index.data(PlayerItemModel::FavoriteRole).toBool()
                                 ? ":/resources/ui/favorite_active.png"
                                 : ":/resources/ui/favorite.png",
                             title.favorite.size()));
    painter.setFont(titleFont);
    painter.setPen(QColor("#ffffff"));
    painter.drawText(title.title, Qt::AlignCenter,
                     QFontMetrics(titleFont).elidedText(name, Qt::ElideRight,
                                                        title.title.width()));

    const QRect middle = masteryMiddleRect(content, scale);
    QFont detailFont = baseFont;
    detailFont.setPixelSize(wfgui::scaled(15, scale));
    detailFont.setWeight(QFont::Light);
    painter.setFont(detailFont);
    painter.setPen(QColor("#ffffff"));
    if (owned) {
      const QString prefix = "Level ";
      const QString rank =
          QString("%1/%2")
              .arg(index.data(PlayerItemModel::RankRole).toInt())
              .arg(index.data(PlayerItemModel::MaxRankRole).toInt());
      const QFontMetrics metrics(detailFont);
      const int width = metrics.horizontalAdvance(prefix + rank);
      const QRect detail(middle.center().x() - width / 2,
                         content.top() + wfgui::scaled(31, scale), width,
                         wfgui::scaled(24, scale));
      painter.drawText(detail, Qt::AlignCenter, prefix + rank);
    } else {
      detailFont.setWeight(QFont::Normal);
      detailFont.setPixelSize(wfgui::scaled(16, scale));
      painter.setFont(detailFont);
      const QString chance = probabilityText(
          index.data(PlayerItemModel::RelicProbabilityRole).toDouble());
      const QVariant priceValue =
          index.data(PlayerItemModel::AcquisitionPlatinumRole);
      const QString priceState =
          index.data(PlayerItemModel::AcquisitionPriceStateRole).toString();
      const QString price = priceState == "loading"
                                ? "..."
                                : (priceState == "ready" && priceValue.isValid()
                                       ? priceValue.toString()
                                       : "--");
      const QFontMetrics metrics(detailFont);
      const int iconSize = wfgui::scaled(28, scale);
      const int iconGap = wfgui::scaled(2, scale);
      const int groupGap = wfgui::scaled(27, scale);
      const int chanceWidth =
          metrics.horizontalAdvance(chance) + iconGap + iconSize;
      const int priceWidth =
          metrics.horizontalAdvance(price) + iconGap + iconSize;
      int x = middle.center().x() - (chanceWidth + groupGap + priceWidth) / 2;
      const int top = content.top() + wfgui::scaled(29, scale);
      const auto drawMetric = [&](const QString &text, const QString &icon,
                                  int width) {
        const int textWidth = metrics.horizontalAdvance(text);
        painter.drawText(QRect(x, top, textWidth, iconSize), Qt::AlignCenter,
                         text);
        const QRect iconRect(x + textWidth + iconGap, top, iconSize, iconSize);
        wfgui::drawContained(
            painter, iconRect,
            wfgui::cachedThumbnail(painter, icon, iconRect.size()));
        x += width;
      };
      drawMetric(chance, ":/resources/activity/axi.png", chanceWidth);
      x += groupGap;
      drawMetric(price, ":/assets/platinum.png", priceWidth);
    }

    const QRect stateArea(content.right() - wfgui::scaled(109, scale) + 1,
                          content.top(), wfgui::scaled(109, scale),
                          content.height());
    if (owned) {
      QFont stateFont = baseFont;
      stateFont.setPixelSize(wfgui::scaled(14, scale));
      stateFont.setWeight(QFont::Light);
      const QFontMetrics metrics(stateFont);
      const int iconSize = wfgui::scaled(20, scale);
      const int gap = wfgui::scaled(4, scale);
      const int width = iconSize + gap + metrics.horizontalAdvance("Owned");
      int x = stateArea.center().x() - width / 2;
      const int y = stateArea.center().y() - iconSize / 2;
      const QRect iconRect(x, y, iconSize, iconSize);
      const QRect glyphRect =
          centeredSquare(iconRect, wfgui::scaled(18, scale));
      wfgui::drawContained(
          painter, iconRect,
          wfgui::cachedThumbnail(painter, ":/resources/ui/owned.png",
                                 glyphRect.size()));
      x += iconSize + gap;
      painter.setFont(stateFont);
      painter.setPen(QColor("#abf5ab"));
      painter.drawText(
          QRect(x, y, metrics.horizontalAdvance("Owned"), iconSize),
          Qt::AlignLeft | Qt::AlignVCenter, "Owned");
    } else {
      paintComponents(painter, content, components,
                      PlayerItemGridWidget::Kind::Mastery, scale, dirtyRegion);
    }
  }

  static void paintComponents(QPainter &painter, const QRect &content,
                              const QVariantList &components,
                              PlayerItemGridWidget::Kind kind, qreal scale,
                              const QRect &dirtyRegion) {
    const QList<QRect> rects = componentRects(
        content, static_cast<int>(components.size()), kind, scale);
    paintComponentRects(painter, components, rects, kind, scale, dirtyRegion);
  }

  static void paintComponentRects(QPainter &painter,
                                  const QVariantList &components,
                                  const QList<QRect> &rects,
                                  PlayerItemGridWidget::Kind kind, qreal scale,
                                  const QRect &dirtyRegion) {
    for (int componentIndex = 0; componentIndex < rects.size();
         ++componentIndex) {
      const QVariantMap component = components.at(componentIndex).toMap();
      const QRect &circle = rects.at(componentIndex);
      const bool builtEnough = component.value("owned").toInt() >=
                               component.value("required").toInt();
      const bool hasBlueprint = component.value("blueprint_owned").toInt() > 0;
      const bool available = builtEnough || hasBlueprint;
      painter.setBrush(available ? QColor(0x6e, 0x9b, 0x68, 0x78)
                                 : QColor(255, 255, 255, 0x12));
      painter.setPen(QPen(available ? QColor("#46d234") : QColor("#566078"),
                          wfgui::scaled(available ? 2 : 1, scale)));
      painter.drawEllipse(circle);
      painter.save();
      QPainterPath clip;
      clip.addEllipse(circle.adjusted(2, 2, -2, -2));
      painter.setClipPath(clip);
      const QRect imageRect = circle.adjusted(2, 2, -2, -2);
      wfgui::drawContained(
          painter, imageRect,
          wfgui::cachedThumbnail(
              painter, component.value("assetRef").value<wfgui::AssetRef>(),
              imageRect.size(), dirtyRegion));
      painter.restore();
      if (component.value("search_match").toBool()) {
        QPen searchPen(QColor("#f0c95a"));
        QRect searchCircle = circle;
        if (kind == PlayerItemGridWidget::Kind::Foundry) {
          searchPen.setWidthF(std::max<qreal>(1.0, scale));
          searchPen.setStyle(Qt::DotLine);
          searchPen.setCapStyle(Qt::RoundCap);
        } else {
          searchPen.setWidth(wfgui::scaled(3, scale));
          const int inset = wfgui::scaled(1, scale);
          searchCircle.adjust(inset, inset, -inset, -inset);
        }
        painter.setPen(searchPen);
        painter.setBrush(Qt::NoBrush);
        painter.drawEllipse(searchCircle);
      }
      if (kind == PlayerItemGridWidget::Kind::Foundry ||
          kind == PlayerItemGridWidget::Kind::Inventory) {
        const int owned = component.value("owned").toInt();
        if (owned > 0 || kind == PlayerItemGridWidget::Kind::Inventory) {
          QFont countFont = painter.font();
          countFont.setPixelSize(wfgui::scaled(13, scale));
          countFont.setWeight(QFont::Normal);
          painter.setFont(countFont);
          const QString count = QString::number(owned);
          const QFontMetrics metrics(countFont);
          const int badgeHeight = wfgui::scaled(14, scale);
          const int badgeWidth = std::max(wfgui::scaled(15, scale),
                                          metrics.horizontalAdvance(count) +
                                              wfgui::scaled(6, scale));
          const QRect badge(
              circle.right() - badgeWidth + wfgui::scaled(3, scale),
              circle.bottom() - badgeHeight + wfgui::scaled(3, scale),
              badgeWidth, badgeHeight);
          painter.setPen(Qt::NoPen);
          painter.setBrush(QColor(0, 0, 0, 0xbd));
          painter.drawRoundedRect(badge, badgeHeight / 2, badgeHeight / 2);
          painter.setPen(QColor("#ffffff"));
          painter.drawText(badge, Qt::AlignCenter, count);
        }
        if (kind == PlayerItemGridWidget::Kind::Foundry &&
            component.value("owned_relic").toBool()) {
          const QRect marker = ownedRelicMarkerRect(circle, scale);
          painter.setPen(QPen(QColor("#cfb020"), wfgui::scaled(2, scale)));
          painter.setBrush(QColor(0x4b, 0x4a, 0x16, 0xb3));
          painter.drawEllipse(marker);
          const QRect imageRect = marker.adjusted(
              wfgui::scaled(2, scale), wfgui::scaled(2, scale),
              -wfgui::scaled(2, scale), -wfgui::scaled(2, scale));
          wfgui::drawContained(
              painter, imageRect,
              wfgui::cachedThumbnail(painter, ":/resources/ui/fissure.png",
                                     imageRect.size()));
        }
      }
    }
  }

  PlayerItemGridWidget::Kind kind_;
  std::function<void(const QString &, const QString &)> marketRequest_;
  std::function<void(const QString &)> relicRequest_;
};
} // namespace

PlayerItemGridWidget::PlayerItemGridWidget(Kind kind, QWidget *parent)
    : QListView(parent), kind_(kind), visibleDataTimer_(new QTimer(this)) {
  setObjectName("playerItemGrid");
  wfgui::setCaptureTarget(this, kind == Kind::Foundry     ? "foundry.grid"
                                : kind == Kind::Inventory ? "inventory.grid"
                                                          : "mastery.grid");
  setItemDelegate(new PlayerItemDelegate(
      kind,
      [this](const QString &item, const QString &side) {
        emit marketItemRequested(item, side);
      },
      [this](const QString &reward) { emit relicRewardRequested(reward); },
      this));
  setViewMode(QListView::IconMode);
  setFlow(QListView::LeftToRight);
  setWrapping(true);
  setResizeMode(QListView::Adjust);
  setLayoutMode(QListView::SinglePass);
  setUniformItemSizes(true);
  setMouseTracking(true);
  setSelectionMode(QAbstractItemView::NoSelection);
  setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  setVerticalScrollBarPolicy(Qt::ScrollBarAlwaysOn);
  setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
  setSpacing(0);
  visibleDataTimer_->setInterval(50);
  visibleDataTimer_->setSingleShot(true);
  connect(visibleDataTimer_, &QTimer::timeout, this,
          &PlayerItemGridWidget::requestVisibleData);
  connect(verticalScrollBar(), &QScrollBar::valueChanged, this,
          [this] { scheduleVisibleData(); });
}

void PlayerItemGridWidget::contextMenuEvent(QContextMenuEvent *event) {
  const QPoint position = viewport()->mapFromGlobal(event->globalPos());
  const ContextTarget target = contextTargetAt(this, kind_, position);
  if (!target.item.isValid()) {
    return;
  }

  auto *menu = new QMenu(this);
  menu->setObjectName("playerItemContextMenu");
  menu->setAttribute(Qt::WA_DeleteOnClose);
  const QString itemName =
      target.item.data(PlayerItemModel::NameRole).toString();
  const bool component = !target.component.isEmpty();

  if (component) {
    const QString componentName = componentMarketName(target.component);
    const QString componentMarket =
        target.component.value("market_name").toString();
    if ((target.item.data(PlayerItemModel::IsPrimeRole).toBool() &&
         target.component.value("relic_drop").toBool()) ||
        target.component.value("owned_relic").toBool()) {
      QAction *relics = menu->addAction("Show in Relic Planner");
      connect(relics, &QAction::triggered, this, [this, componentName] {
        emit relicRewardRequested(componentName);
      });
    } else {
      const QString itemRelics = relicSearchTerm(target.item);
      if (!itemRelics.isEmpty()) {
        QAction *relics = menu->addAction("Show parts in Relic Planner");
        connect(relics, &QAction::triggered, this,
                [this, itemRelics] { emit relicRewardRequested(itemRelics); });
      }
    }
    if (kind_ == Kind::Mastery &&
        target.item.data(PlayerItemModel::HasRecipeRole).toBool()) {
      QAction *foundry = menu->addAction("Show item in Foundry");
      connect(foundry, &QAction::triggered, this,
              [this, itemName] { emit foundryItemRequested(itemName); });
    }
    if (!componentMarket.isEmpty()) {
      if (!menu->actions().isEmpty()) {
        menu->addSeparator();
      }
      QAction *sell = menu->addAction("View WTS listings");
      QAction *buy = menu->addAction("View WTB listings");
      connect(sell, &QAction::triggered, this, [this, componentMarket] {
        emit marketItemRequested(componentMarket, "sell");
      });
      connect(buy, &QAction::triggered, this, [this, componentMarket] {
        emit marketItemRequested(componentMarket, "buy");
      });
    }
  } else {
    const QString relics = relicSearchTerm(target.item);
    if (!relics.isEmpty()) {
      QAction *showRelics = menu->addAction(
          target.item.data(PlayerItemModel::GroupRole).toString() == "relics"
              ? "Open in Relic Planner"
              : "Show parts in Relic Planner");
      connect(showRelics, &QAction::triggered, this,
              [this, relics] { emit relicRewardRequested(relics); });
    }
    if (kind_ == Kind::Mastery &&
        target.item.data(PlayerItemModel::HasRecipeRole).toBool()) {
      QAction *foundry = menu->addAction("Show item in Foundry");
      connect(foundry, &QAction::triggered, this,
              [this, itemName] { emit foundryItemRequested(itemName); });
    }
    if (target.item.data(PlayerItemModel::TradableRole).toBool()) {
      const QString marketName =
          target.item.data(PlayerItemModel::MarketNameRole).toString();
      if (!marketName.isEmpty()) {
        if (!menu->actions().isEmpty()) {
          menu->addSeparator();
        }
        QAction *sell = menu->addAction("View WTS listings");
        QAction *buy = menu->addAction("View WTB listings");
        connect(sell, &QAction::triggered, this, [this, marketName] {
          emit marketItemRequested(marketName, "sell");
        });
        connect(buy, &QAction::triggered, this, [this, marketName] {
          emit marketItemRequested(marketName, "buy");
        });
      }
    }
  }

  if (menu->actions().isEmpty()) {
    menu->deleteLater();
    return;
  }
  menu->popup(event->globalPos());
  event->accept();
}

void PlayerItemGridWidget::setModel(QAbstractItemModel *itemModel) {
  if (model()) {
    disconnect(model(), nullptr, this, nullptr);
  }
  clearScrollAnchor();
  QListView::setModel(itemModel);
  if (itemModel) {
    const auto request = [this] { scheduleVisibleData(); };
    connect(itemModel, &QAbstractItemModel::modelAboutToBeReset, this,
            &PlayerItemGridWidget::preserveScrollPosition);
    connect(itemModel, &QAbstractItemModel::layoutAboutToBeChanged, this,
            &PlayerItemGridWidget::preserveScrollPosition);
    connect(itemModel, &QAbstractItemModel::rowsAboutToBeInserted, this,
            &PlayerItemGridWidget::preserveScrollPosition);
    connect(itemModel, &QAbstractItemModel::rowsAboutToBeRemoved, this,
            &PlayerItemGridWidget::preserveScrollPosition);
    connect(itemModel, &QAbstractItemModel::modelReset, this, request);
    connect(itemModel, &QAbstractItemModel::rowsInserted, this, request);
    connect(itemModel, &QAbstractItemModel::rowsRemoved, this, request);
    connect(itemModel, &QAbstractItemModel::layoutChanged, this, request);
    connect(itemModel, &QAbstractItemModel::modelReset, this,
            &PlayerItemGridWidget::restoreScrollPosition);
    connect(itemModel, &QAbstractItemModel::layoutChanged, this,
            &PlayerItemGridWidget::restoreScrollPosition);
    connect(itemModel, &QAbstractItemModel::rowsInserted, this,
            &PlayerItemGridWidget::restoreScrollPosition);
    connect(itemModel, &QAbstractItemModel::rowsRemoved, this,
            &PlayerItemGridWidget::restoreScrollPosition);
  }
}

void PlayerItemGridWidget::showEvent(QShowEvent *event) {
  QListView::showEvent(event);
  scheduleVisibleData();
}

void PlayerItemGridWidget::preserveScrollPosition() {
  if (scrollAnchorPending_ || scrollRestoreScheduled_ || !model() ||
      model()->rowCount() == 0) {
    return;
  }

  const auto [first, last] = visibleRows();
  const QRect viewportRect = viewport()->rect();
  QModelIndex anchor;
  QRect anchorRect;
  for (int row = first; row <= last; ++row) {
    const QModelIndex candidate = model()->index(row, 0);
    const QRect rect = visualRect(candidate);
    if (!rect.isValid() || !rect.intersects(viewportRect)) {
      continue;
    }
    if (!anchor.isValid() || rect.top() < anchorRect.top() ||
        (rect.top() == anchorRect.top() && rect.left() < anchorRect.left())) {
      anchor = candidate;
      anchorRect = rect;
    }
  }
  if (!anchor.isValid()) {
    return;
  }

  preservedAnchorId_ = anchor.data(PlayerItemModel::IdRole).toString();
  preservedAnchorOffset_ = anchorRect.top();
  preservedScrollValue_ = verticalScrollBar()->value();
  scrollAnchorPending_ = !preservedAnchorId_.isEmpty();
}

void PlayerItemGridWidget::restoreScrollPosition() {
  if (!scrollAnchorPending_ || scrollRestoreScheduled_) {
    return;
  }
  scrollRestoreScheduled_ = true;
  QTimer::singleShot(0, this, [this] {
    doItemsLayout();
    QModelIndex anchor;
    if (model()) {
      for (int row = 0; row < model()->rowCount(); ++row) {
        const QModelIndex candidate = model()->index(row, 0);
        if (candidate.data(PlayerItemModel::IdRole).toString() ==
            preservedAnchorId_) {
          anchor = candidate;
          break;
        }
      }
    }

    if (anchor.isValid()) {
      scrollTo(anchor, QAbstractItemView::PositionAtTop);
      doItemsLayout();
      const int delta = visualRect(anchor).top() - preservedAnchorOffset_;
      verticalScrollBar()->setValue(std::clamp(
          verticalScrollBar()->value() + delta, verticalScrollBar()->minimum(),
          verticalScrollBar()->maximum()));
    } else {
      verticalScrollBar()->setValue(std::clamp(preservedScrollValue_,
                                               verticalScrollBar()->minimum(),
                                               verticalScrollBar()->maximum()));
    }
    clearScrollAnchor();
  });
}

void PlayerItemGridWidget::clearScrollAnchor() {
  preservedAnchorId_.clear();
  preservedAnchorOffset_ = 0;
  preservedScrollValue_ = 0;
  scrollAnchorPending_ = false;
  scrollRestoreScheduled_ = false;
}

void PlayerItemGridWidget::resizeEvent(QResizeEvent *event) {
  QListView::resizeEvent(event);
  updateGrid();
}

void PlayerItemGridWidget::updateGrid() {
  const qreal scale = wfgui::displayScale(this);
  const int width = std::max(1, viewport()->width());
  const int gap = wfgui::scaled(Gap, scale);
  const int minimumTrack = wfgui::scaled(
      kind_ == Kind::Foundry
          ? FoundryMinTrack
          : (kind_ == Kind::Inventory ? InventoryMinTrack : MasteryMinTrack),
      scale);
  const int columns = std::max(1, (width + gap) / (minimumTrack + gap));
  const int track =
      std::min(wfgui::scaled(MaxTrack, scale),
               std::max(1, (width - 1 - columns * gap) / columns));
  const int height = wfgui::scaled(kind_ == Kind::Foundry
                                       ? FoundryCardHeight
                                       : (kind_ == Kind::Mastery ? 91 : 130),
                                   scale);
  setGridSize({track + gap, height + gap});
  scheduleDelayedItemsLayout();
  scheduleVisibleData();
}

void PlayerItemGridWidget::refreshVisibleQuotes() {
  requestVisibleQuotes(true);
}

void PlayerItemGridWidget::requestAllQuotes(bool refresh) {
  if (model()) {
    requestQuotes(0, model()->rowCount() - 1, refresh);
  }
}

void PlayerItemGridWidget::scheduleVisibleData() { visibleDataTimer_->start(); }

void PlayerItemGridWidget::requestVisibleData() {
  requestVisibleAssets();
  requestVisibleQuotes();
}

void PlayerItemGridWidget::requestVisibleAssets() {
  if (!model() || gridSize().height() <= 0 || gridSize().width() <= 0) {
    return;
  }
  QJsonArray assets;
  const auto [first, last] = visibleRows();
  for (int row = first; row <= last; ++row) {
    const QModelIndex item = model()->index(row, 0);
    const QVariantMap asset = item.data(PlayerItemModel::AssetSpecRole).toMap();
    if (!asset.value("id").toString().isEmpty()) {
      assets.append(QJsonObject::fromVariantMap(asset));
    }
    for (const QVariant &value :
         item.data(PlayerItemModel::ComponentsRole).toList()) {
      const QVariantMap componentAsset = value.toMap().value("asset").toMap();
      if (!componentAsset.value("id").toString().isEmpty()) {
        assets.append(QJsonObject::fromVariantMap(componentAsset));
      }
    }
  }
  if (!assets.isEmpty()) {
    emit assetsNeeded(assets);
  }
}

void PlayerItemGridWidget::requestVisibleQuotes(bool refresh) {
  if ((kind_ != Kind::Inventory && kind_ != Kind::Mastery) || !model()) {
    return;
  }
  const auto [first, last] = visibleRows();
  requestQuotes(first, last, refresh);
}

void PlayerItemGridWidget::requestQuotes(int first, int last, bool refresh) {
  if ((kind_ != Kind::Inventory && kind_ != Kind::Mastery) || !model()) {
    return;
  }
  QStringList items;
  for (int row = first; row <= last; ++row) {
    const QModelIndex item = model()->index(row, 0);
    if (kind_ == Kind::Inventory &&
        item.data(PlayerItemModel::TradableRole).toBool()) {
      items.append(item.data(PlayerItemModel::MarketNameRole).toString());
    } else if (kind_ == Kind::Mastery &&
               !item.data(PlayerItemModel::OwnedRole).toBool() &&
               (refresh || item.data(PlayerItemModel::AcquisitionPriceStateRole)
                                   .toString() == "loading")) {
      for (const QVariant &value :
           item.data(PlayerItemModel::ComponentsRole).toList()) {
        const QVariantMap component = value.toMap();
        const QString marketName = component.value("market_name").toString();
        if (!marketName.isEmpty() && component.value("owned").toInt() <
                                         component.value("required").toInt()) {
          items.append(marketName);
        }
      }
    }
  }
  items.removeDuplicates();
  if (!items.isEmpty()) {
    emit quotesNeeded(items, refresh);
  }
}

std::pair<int, int> PlayerItemGridWidget::visibleRows() const {
  if (!model() || gridSize().height() <= 0 || gridSize().width() <= 0) {
    return {0, -1};
  }
  const int columns = std::max(1, viewport()->width() / gridSize().width());
  const int firstLine =
      std::max(0, verticalScrollBar()->value() / gridSize().height() - 1);
  const int lastLine = (verticalScrollBar()->value() + viewport()->height()) /
                           gridSize().height() +
                       1;
  return {firstLine * columns,
          std::min(model()->rowCount() - 1, (lastLine + 1) * columns - 1)};
}
