#include "relic_grid_widget.h"

#include <QAbstractItemModel>
#include <QContextMenuEvent>
#include <QFontMetrics>
#include <QHelpEvent>
#include <QMenu>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QResizeEvent>
#include <QStyleOptionViewItem>
#include <QStyledItemDelegate>
#include <QVariantList>
#include <QVariantMap>

#include <algorithm>

#include "display_metrics.h"
#include "image_cache.h"
#include "relic_card_layout.h"
#include "relic_model.h"
#include "tooltip.h"
#include "widget_capture.h"

namespace {
constexpr int CardHeight = 108;
constexpr int MaximumTrackWidth = 620;

QRect overscan(const QRect &rect, int percent) {
  const QSize size(rect.width() * percent / 100, rect.height() * percent / 100);
  return {QPoint(rect.center().x() - size.width() / 2,
                 rect.center().y() - size.height() / 2),
          size};
}

QRect cardBounds(const QRect &item, qreal scale) {
  const int width =
      std::min(item.width(), wfgui::scaled(MaximumTrackWidth, scale));
  return {item.center().x() - width / 2, item.top(), width, item.height()};
}

wfgui::RelicCardLayout cardLayout(const RelicGridWidget *view,
                                  const QModelIndex &index) {
  const QVariantList refinements =
      index.data(RelicModel::RefinementsRole).toList();
  const QVariantList rewards = index.data(RelicModel::RewardsRole).toList();
  const qreal scale = wfgui::displayScale(view);
  return wfgui::RelicCardLayout::calculate(
      cardBounds(view->visualRect(index), scale),
      static_cast<int>(refinements.size()), static_cast<int>(rewards.size()),
      scale);
}

int rewardIndexAt(const RelicGridWidget *view, const QModelIndex &index,
                  const QPoint &position) {
  const QVariantList rewards = index.data(RelicModel::RewardsRole).toList();
  const auto layout = cardLayout(view, index);
  const int count = std::min(static_cast<int>(layout.rewardCells.size()),
                             static_cast<int>(rewards.size()));
  for (int reward = 0; reward < count; ++reward) {
    if (layout.rewardCells.at(reward).contains(position)) {
      return reward;
    }
  }
  return -1;
}

class RelicCardDelegate final : public QStyledItemDelegate {
public:
  explicit RelicCardDelegate(QObject *parent) : QStyledItemDelegate(parent) {}

  QSize sizeHint(const QStyleOptionViewItem &,
                 const QModelIndex &) const override {
    const auto *view = qobject_cast<const QListView *>(parent());
    return view ? view->gridSize() : QSize(520, CardHeight);
  }

  void paint(QPainter *painter, const QStyleOptionViewItem &option,
             const QModelIndex &index) const override {
    painter->save();
    painter->setRenderHint(QPainter::Antialiasing);
    painter->setRenderHint(QPainter::SmoothPixmapTransform);

    const qreal scale =
        wfgui::displayScale(qobject_cast<const QWidget *>(parent()));
    const QVariantList refinements =
        index.data(RelicModel::RefinementsRole).toList();
    const QVariantList rewards = index.data(RelicModel::RewardsRole).toList();
    const wfgui::RelicCardLayout layout = wfgui::RelicCardLayout::calculate(
        cardBounds(option.rect, scale), static_cast<int>(refinements.size()),
        static_cast<int>(rewards.size()), scale);
    const QRect &card = layout.card;
    painter->setPen(Qt::NoPen);
    painter->setBrush(option.state & QStyle::State_MouseOver
                          ? QColor("#272f47")
                          : QColor("#20283e"));
    painter->drawRoundedRect(card, wfgui::scaled(12, scale),
                             wfgui::scaled(12, scale));

    const QPixmap relicPixmap = wfgui::cachedThumbnail(
        *painter,
        index.data(RelicModel::RelicAssetRole).value<wfgui::AssetRef>(),
        layout.relicArt.size(), option.rect);
    if (relicPixmap.isNull()) {
      painter->setPen(QPen(QColor("#8a8c95"), 1.5 * scale));
      const QRectF glyph = layout.image.adjusted(
          wfgui::scaled(17, scale), wfgui::scaled(11, scale),
          -wfgui::scaled(17, scale), -wfgui::scaled(11, scale));
      painter->drawEllipse(glyph);
      QPainterPath diamond;
      diamond.moveTo(glyph.center().x(), glyph.top());
      diamond.lineTo(glyph.right(), glyph.center().y());
      diamond.lineTo(glyph.center().x(), glyph.bottom());
      diamond.lineTo(glyph.left(), glyph.center().y());
      diamond.closeSubpath();
      painter->drawPath(diamond);
    } else {
      wfgui::drawContained(*painter, layout.relicArt, relicPixmap);
    }

    QFont badgeFont = option.font;
    badgeFont.setPixelSize(wfgui::scaled(15, scale));
    badgeFont.setWeight(QFont::DemiBold);
    painter->setFont(badgeFont);
    painter->setPen(QColor("#ffffff"));
    painter->setBrush(Qt::NoBrush);
    painter->drawText(
        layout.amountBadge, Qt::AlignCenter,
        QString("x%1").arg(index.data(RelicModel::AmountOwnedRole).toInt()));

    if (index.data(RelicModel::VaultedRole).toBool()) {
      const QRect badge = overscan(layout.vaultedBadge, 125);
      wfgui::drawContained(*painter, badge,
                           wfgui::cachedThumbnail(
                               *painter, ":/assets/vaulted.png", badge.size()));
    }

    QFont nameFont = option.font;
    nameFont.setPixelSize(wfgui::scaled(19, scale));
    nameFont.setWeight(QFont::Medium);
    painter->setFont(nameFont);
    painter->setPen(QColor("#ffffff"));
    const QString name = index.data(RelicModel::NameRole).toString();
    painter->drawText(layout.title, Qt::AlignLeft | Qt::AlignVCenter,
                      QFontMetrics(nameFont).elidedText(name, Qt::ElideRight,
                                                        layout.title.width()));

    const QRect platinumHeader = layout.platinumHeader.adjusted(
        wfgui::scaled(8, scale), wfgui::scaled(2, scale),
        -wfgui::scaled(8, scale), -wfgui::scaled(2, scale));
    const QRect ducatHeader = layout.ducatHeader.adjusted(
        wfgui::scaled(8, scale), wfgui::scaled(2, scale),
        -wfgui::scaled(8, scale), -wfgui::scaled(2, scale));
    wfgui::drawContained(*painter, platinumHeader,
                         wfgui::cachedThumbnail(*painter,
                                                ":/assets/platinum.png",
                                                platinumHeader.size()));
    wfgui::drawContained(*painter, ducatHeader,
                         wfgui::cachedThumbnail(*painter, ":/assets/ducats.png",
                                                ducatHeader.size()));

    const int rows = static_cast<int>(layout.refinementRows.size());
    QFont valueFont = option.font;
    valueFont.setPointSizeF((rows >= 4 ? 9.0 : 10.0) * scale);
    painter->setFont(valueFont);
    for (int row = 0; row < rows; ++row) {
      const QVariantMap refinement = row < refinements.size()
                                         ? refinements.at(row).toMap()
                                         : QVariantMap{};
      const auto &rowLayout = layout.refinementRows.at(row);
      painter->setPen(QColor("#ffffff"));
      const QString label =
          refinement.isEmpty()
              ? QString()
              : QString("%1x %2")
                    .arg(refinement.value("amountOwned").toInt())
                    .arg(refinement.value("name").toString());
      painter->drawText(rowLayout.label, Qt::AlignLeft | Qt::AlignVCenter,
                        QFontMetrics(valueFont).elidedText(
                            label, Qt::ElideRight, rowLayout.label.width()));
      const bool hasPrice = refinement.value("hasPrice").toBool();
      const bool complete = refinement.value("priceComplete").toBool();
      painter->setPen(hasPrice ? QColor("#ffffff") : QColor("#8a8c95"));
      painter->drawText(
          rowLayout.platinum, Qt::AlignCenter,
          hasPrice ? QString("%1%2")
                         .arg(complete ? QString() : QString("~"))
                         .arg(refinement.value("expectedPlatinum").toInt())
                   : QString("--"));
      painter->setPen(QColor("#ffffff"));
      painter->drawText(
          rowLayout.ducats, Qt::AlignCenter,
          QString::number(refinement.value("expectedDucats").toInt()));
    }

    for (int rewardIndex = 0;
         rewardIndex < static_cast<int>(layout.rewardCells.size());
         ++rewardIndex) {
      const QVariantMap reward = rewards.at(rewardIndex).toMap();
      const QRect &rewardRect = layout.rewardCells.at(rewardIndex);
      const bool owned = reward.value("owned").toInt() > 0;
      painter->setBrush(owned ? QColor(0x6e, 0x9b, 0x68, 0x78)
                              : QColor(255, 255, 255, 0x12));
      painter->setPen(Qt::NoPen);
      painter->drawEllipse(rewardRect);
      const int imageInset = wfgui::scaled(2, scale);
      const QRect rewardImageRect =
          rewardRect.adjusted(imageInset, imageInset, -imageInset, -imageInset);
      const QPixmap rewardImage = wfgui::cachedThumbnail(
          *painter, reward.value("assetRef").value<wfgui::AssetRef>(),
          rewardImageRect.size(), option.rect);
      if (rewardImage.isNull()) {
        painter->setPen(QPen(QColor("#8a8c95"), wfgui::scaled(1, scale)));
        const int placeholderInset = wfgui::scaled(9, scale);
        painter->drawEllipse(
            rewardRect.adjusted(placeholderInset, placeholderInset,
                                -placeholderInset, -placeholderInset));
      } else {
        painter->save();
        QPainterPath clip;
        clip.addEllipse(rewardImageRect);
        painter->setClipPath(clip);
        wfgui::drawContained(*painter, rewardImageRect, rewardImage);
        painter->restore();
      }
      if (owned) {
        painter->setPen(QPen(QColor("#46d234"), wfgui::scaled(2, scale)));
        painter->setBrush(Qt::NoBrush);
        const int ownedInset = wfgui::scaled(2, scale);
        painter->drawEllipse(rewardRect.adjusted(ownedInset, ownedInset,
                                                 -ownedInset, -ownedInset));
      }
      if (reward.value("search_match").toBool()) {
        const int inset = wfgui::scaled(1, scale);
        painter->setPen(QPen(QColor("#f0c95a"), wfgui::scaled(3, scale)));
        painter->setBrush(Qt::NoBrush);
        painter->drawEllipse(rewardRect.adjusted(inset, inset, -inset, -inset));
      }
    }
    painter->restore();
  }
};
} // namespace

RelicGridWidget::RelicGridWidget(QWidget *parent) : QListView(parent) {
  setObjectName("relicGrid");
  wfgui::setCaptureTarget(this, "relic-planner.grid");
  setItemDelegate(new RelicCardDelegate(this));
  setViewMode(QListView::IconMode);
  setFlow(QListView::LeftToRight);
  setWrapping(true);
  setResizeMode(QListView::Adjust);
  setLayoutMode(QListView::SinglePass);
  setMovement(QListView::Static);
  setUniformItemSizes(true);
  setMouseTracking(true);
  setSelectionMode(QAbstractItemView::NoSelection);
  setEditTriggers(QAbstractItemView::NoEditTriggers);
  setFrameShape(QFrame::NoFrame);
  setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  setVerticalScrollBarPolicy(Qt::ScrollBarAlwaysOn);
  setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
  setSpacing(0);
}

void RelicGridWidget::setModel(QAbstractItemModel *itemModel) {
  QListView::setModel(itemModel);
  updateGrid();
}

bool RelicGridWidget::viewportEvent(QEvent *event) {
  if (event->type() != QEvent::ToolTip) {
    return QListView::viewportEvent(event);
  }

  auto *help = static_cast<QHelpEvent *>(event);
  const QModelIndex index = indexAt(help->pos());
  if (!index.isValid()) {
    return QListView::viewportEvent(event);
  }
  const QVariantList rewards = index.data(RelicModel::RewardsRole).toList();
  const auto layout = cardLayout(this, index);
  const int reward = rewardIndexAt(this, index, help->pos());
  if (reward >= 0 && reward < rewards.size()) {
    wfgui::showTooltip(viewport(), help->pos(),
                       rewards.at(reward).toMap().value("name").toString(),
                       layout.rewardCells.at(reward));
  } else {
    wfgui::showTooltip(viewport(), help->pos(),
                       index.data(RelicModel::NameRole).toString(),
                       layout.card);
  }
  return true;
}

void RelicGridWidget::mouseReleaseEvent(QMouseEvent *event) {
  if (event->button() != Qt::LeftButton) {
    QListView::mouseReleaseEvent(event);
    return;
  }

  const QPoint position = event->position().toPoint();
  const QModelIndex index = indexAt(position);
  if (!index.isValid()) {
    QListView::mouseReleaseEvent(event);
    return;
  }
  const QVariantList rewards = index.data(RelicModel::RewardsRole).toList();
  const int reward = rewardIndexAt(this, index, position);
  if (reward >= 0 && reward < rewards.size()) {
    const QVariantMap data = rewards.at(reward).toMap();
    emit marketItemRequested(data.value("name").toString(),
                             data.value("owned").toInt() > 0 ? "buy" : "sell");
    event->accept();
    return;
  }

  const auto layout = cardLayout(this, index);
  if (layout.image.contains(position) || layout.title.contains(position)) {
    emit marketItemRequested(
        index.data(RelicModel::NameRole).toString(),
        index.data(RelicModel::AmountOwnedRole).toInt() > 0 ? "buy" : "sell");
    event->accept();
    return;
  }
  QListView::mouseReleaseEvent(event);
}

void RelicGridWidget::contextMenuEvent(QContextMenuEvent *event) {
  const QPoint position = viewport()->mapFromGlobal(event->globalPos());
  const QModelIndex index = indexAt(position);
  if (!index.isValid()) {
    return;
  }

  const QVariantList rewards = index.data(RelicModel::RewardsRole).toList();
  const int reward = rewardIndexAt(this, index, position);
  const bool rewardHit = reward >= 0 && reward < rewards.size();
  const QString item = rewardHit
                           ? rewards.at(reward).toMap().value("name").toString()
                           : index.data(RelicModel::NameRole).toString();
  if (item.isEmpty()) {
    return;
  }

  auto *menu = new QMenu(this);
  menu->setObjectName("relicContextMenu");
  menu->setAttribute(Qt::WA_DeleteOnClose);
  const bool primeReward =
      rewardHit && item.contains(" Prime ", Qt::CaseInsensitive);
  if (rewardHit) {
    QAction *filter = menu->addAction("Show all relics containing this reward");
    connect(filter, &QAction::triggered, this,
            [this, item] { emit rewardFilterRequested(item); });
    if (primeReward) {
      QAction *foundry = menu->addAction("Show item in Foundry");
      connect(foundry, &QAction::triggered, this,
              [this, item] { emit foundryItemRequested(item); });
    }
  }
  if (!rewardHit || primeReward) {
    if (!menu->actions().isEmpty()) {
      menu->addSeparator();
    }
    QAction *sell = menu->addAction("View WTS listings");
    QAction *buy = menu->addAction("View WTB listings");
    connect(sell, &QAction::triggered, this,
            [this, item] { emit marketItemRequested(item, "sell"); });
    connect(buy, &QAction::triggered, this,
            [this, item] { emit marketItemRequested(item, "buy"); });
  }
  menu->popup(event->globalPos());
  event->accept();
}

void RelicGridWidget::resizeEvent(QResizeEvent *event) {
  QListView::resizeEvent(event);
  updateGrid();
}

void RelicGridWidget::updateGrid() {
  const qreal scale = wfgui::displayScale(this);
  const int width = std::max(1, viewport()->width());
  const int columns = wfgui::relicGridColumns(width, scale);
  const QSize size(std::max(1, width / columns),
                   wfgui::scaled(CardHeight, scale));
  if (gridSize() == size && scale_ == scale) {
    return;
  }
  scale_ = scale;
  setGridSize(size);
  scheduleDelayedItemsLayout();
}
