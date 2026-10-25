#include "market_order_card.h"

#include <QGraphicsOpacityEffect>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QPixmap>
#include <QSignalBlocker>
#include <QSizePolicy>
#include <QStyle>
#include <QToolButton>
#include <QVBoxLayout>

#include <initializer_list>
#include <utility>

#include "thumbnail_widget.h"
#include "widget_capture.h"

namespace {
void invokeAction(std::function<void()> action) {
  // A synchronous model update can replace the card's callbacks during this
  // call.
  if (action) {
    action();
  }
}

int firstInt(const QJsonObject &object,
             std::initializer_list<const char *> keys, int fallback = 0) {
  for (const char *key : keys) {
    if (object.value(key).isDouble()) {
      return object.value(key).toInt();
    }
  }
  return fallback;
}

QString extraOrderData(const QJsonObject &order) {
  QStringList values;
  const QString subtype = order.value("subtype").toString();
  if (!subtype.isEmpty()) {
    values.append(subtype.left(1).toUpper() + subtype.mid(1));
  }
  if (order.value("rank").isDouble()) {
    values.append(QString("Rank %1").arg(order.value("rank").toInt()));
  }
  if (order.value("charges").isDouble()) {
    values.append(QString("%1 charges").arg(order.value("charges").toInt()));
  }
  if (order.value("amberStars").isDouble()) {
    values.append(QString("%1 amber").arg(order.value("amberStars").toInt()));
  }
  if (order.value("cyanStars").isDouble()) {
    values.append(QString("%1 cyan").arg(order.value("cyanStars").toInt()));
  }
  const int perTrade = firstInt(order, {"perTrade", "per_trade"}, 1);
  if (perTrade > 1) {
    values.append(QString("%1 per trade").arg(perTrade));
  }
  return values.join(" · ");
}

ThumbnailWidget *iconLabel(const QString &path, const QSize &size,
                           QWidget *parent) {
  auto *label = new ThumbnailWidget(parent);
  label->setFixedSize(size);
  label->setAsset(wfgui::AssetRef::embedded(path, path));
  return label;
}

QWidget *metricCell(QLabel *&text, const QString &iconPath,
                    const QSize &iconSize, QWidget *parent) {
  auto *cell = new QWidget(parent);
  cell->setObjectName("marketMetric");
  auto *layout = new QHBoxLayout(cell);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(5);
  layout->setAlignment(Qt::AlignCenter);
  text = new QLabel(cell);
  text->setObjectName("marketMetricText");
  layout->addWidget(text);
  layout->addWidget(iconLabel(iconPath, iconSize, cell));
  return cell;
}

QToolButton *actionButton(const char *name, const QString &iconPath,
                          const QString &tooltip,
                          const std::function<void()> &action,
                          QWidget *parent) {
  auto *button = new QToolButton(parent);
  button->setObjectName(name);
  button->setProperty("marketAction", true);
  button->setFixedSize(28, 28);
  button->setIcon(QIcon(iconPath));
  button->setIconSize(QSize(21, 21));
  button->setToolTip(tooltip);
  QObject::connect(button, &QToolButton::clicked, button, action);
  return button;
}
} // namespace

MarketOrderCard::MarketOrderCard(const QJsonObject &order,
                                 const QJsonObject &item,
                                 const QJsonObject &quote, int owned,
                                 MarketOrderCardActions actions,
                                 QWidget *parent)
    : QWidget(parent) {

  setObjectName("marketOrderCard");
  wfgui::setCaptureItem(this);
  setMinimumWidth(370);
  setFixedHeight(107);
  setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);

  auto *root = new QVBoxLayout(this);
  root->setContentsMargins(0, 0, 0, 0);
  root->setSpacing(0);

  auto *top = top_ = new QWidget(this);
  top->setObjectName("marketOrderTop");
  top->setFixedHeight(28);
  auto *topLayout = new QHBoxLayout(top);
  topLayout->setContentsMargins(0, 0, 7, 0);
  topLayout->setSpacing(0);

  auto *visibility = visibility_ = new QToolButton(top);
  visibility->setObjectName("marketVisibility");
  visibility->setFixedSize(37, 28);
  visibility->setCheckable(true);
  QIcon visibilityIcon;
  visibilityIcon.addFile(":/resources/market/wfgui-eye-off.png", QSize(),
                         QIcon::Normal, QIcon::Off);
  visibilityIcon.addFile(":/resources/market/wfgui-eye.png", QSize(),
                         QIcon::Normal, QIcon::On);
  visibility->setIcon(visibilityIcon);
  visibility->setIconSize(QSize(20, 20));
  topLayout->addWidget(visibility);

  auto *name = name_ = new QLabel(top);
  name->setObjectName("marketOrderName");
  name->setTextInteractionFlags(Qt::TextSelectableByMouse);
  topLayout->addWidget(name, 1);

  auto *ownedLabel = owned_ = new QLabel(top);
  ownedLabel->setObjectName("marketOrderOwned");
  topLayout->addWidget(ownedLabel);
  {
    auto *warning = warning_ = new QLabel("!", top);
    warning->setObjectName("marketWarning");
    warning->setAlignment(Qt::AlignCenter);
    warning->setFixedSize(37, 28);
    warning->setToolTip("Order quantity exceeds owned quantity");
    topLayout->addWidget(warning);
  }
  root->addWidget(top);

  auto *body = body_ = new QWidget(this);
  body->setObjectName("marketOrderBody");
  body->setFixedHeight(79);
  opacity_ = new QGraphicsOpacityEffect(body);
  opacity_->setOpacity(0.6);
  body->setGraphicsEffect(opacity_);
  auto *bodyLayout = new QHBoxLayout(body);
  bodyLayout->setContentsMargins(0, 0, 9, 0);
  bodyLayout->setSpacing(0);

  auto *art = new QWidget(body);
  art->setObjectName("marketOrderArt");
  art->setFixedWidth(70);
  auto *artLayout = new QVBoxLayout(art);
  artLayout->setContentsMargins(0, 2, 0, 4);
  artLayout->setSpacing(0);
  auto *image = image_ = new ThumbnailWidget(art);
  image->setObjectName("marketOrderImage");
  image->setImageBounds(QSize(44, 44));
  artLayout->addWidget(image, 1);
  auto *side = side_ = new QLabel(art);
  side->setAlignment(Qt::AlignCenter);
  side->setFixedSize(46, 20);
  artLayout->addWidget(side, 0, Qt::AlignHCenter);
  bodyLayout->addWidget(art);

  auto *details = new QWidget(body);
  details->setObjectName("marketOrderDetails");
  auto *grid = new QGridLayout(details);
  grid->setContentsMargins(5, 0, 0, 0);
  grid->setHorizontalSpacing(0);
  grid->setVerticalSpacing(0);
  for (int column = 0; column < 3; ++column) {
    grid->setColumnStretch(column, 1);
  }
  grid->setRowStretch(0, 1);
  grid->setRowStretch(1, 1);

  grid->addWidget(metricCell(quantity_, ":/resources/market/wfgui-boxes.png",
                             QSize(20, 20), details),
                  0, 0);
  grid->addWidget(
      metricCell(price_, ":/assets/platinum.png", QSize(20, 18), details), 0,
      1);
  auto *extra = extra_ = new QLabel(details);
  extra->setObjectName("marketOrderExtra");
  extra->setAlignment(Qt::AlignCenter);
  grid->addWidget(extra, 0, 2);

  auto *lowest = new QWidget(details);
  lowest->setObjectName("marketLowestPrice");
  auto *lowestLayout = new QHBoxLayout(lowest);
  lowestLayout->setContentsMargins(0, 0, 0, 3);
  lowestLayout->setSpacing(4);
  lowestLayout->setAlignment(Qt::AlignCenter);
  lowestLayout->addWidget(actionButton(
      "marketListings", ":/resources/market/wfgui-search.png", "Open listings",
      [this] { invokeAction(actions_.listings); }, lowest));
  auto *lowestTitle = comparisonTitle_ = new QLabel(lowest);
  lowestTitle->setObjectName("marketLowestLabel");
  lowestLayout->addWidget(lowestTitle);
  auto *lowestValue = comparison_ = new QLabel(lowest);
  lowestValue->setObjectName("marketMetricText");
  lowestLayout->addWidget(lowestValue);
  lowestLayout->addWidget(
      iconLabel(":/assets/platinum.png", QSize(20, 18), lowest));
  grid->addWidget(lowest, 1, 0, 1, 2);

  auto *buttons = new QWidget(details);
  buttons->setObjectName("marketOrderActions");
  auto *buttonLayout = new QHBoxLayout(buttons);
  buttonLayout->setContentsMargins(11, 0, 11, 3);
  buttonLayout->setSpacing(2);
  buttonLayout->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
  buttonLayout->addWidget(actionButton(
      "marketEdit", ":/resources/market/wfgui-pencil.png", "Edit order",
      [this] { invokeAction(actions_.edit); }, buttons));
  auto *add = actionButton(
      "marketAdd", ":/resources/market/wfgui-plus.png", "Add one trade unit",
      [this] { invokeAction(actions_.add); }, buttons);
  add->setIconSize(QSize(28, 28));
  buttonLayout->addWidget(add);
  buttonLayout->addWidget(actionButton(
      "marketSold", ":/resources/market/wfgui-check.png", "Mark one sold",
      [this] { invokeAction(actions_.close); }, buttons));
  buttonLayout->addWidget(actionButton(
      "marketDelete", ":/resources/market/wfgui-trash.png", "Delete order",
      [this] { invokeAction(actions_.remove); }, buttons));
  grid->addWidget(buttons, 1, 2);

  bodyLayout->addWidget(details, 1);
  root->addWidget(body);

  connect(visibility, &QToolButton::clicked, this,
          [this] { invokeAction(actions_.visibility); });
  updateOrder(order, item, quote, owned, std::move(actions));
}

void MarketOrderCard::setAsset(const wfgui::AssetRef &asset) {
  image_->setAsset(asset);
}

void MarketOrderCard::updateOrder(const QJsonObject &order,
                                  const QJsonObject &item,
                                  const QJsonObject &quote, int owned,
                                  MarketOrderCardActions actions) {
  actions_ = std::move(actions);
  if (order_ == order && item_ == item && quote_ == quote &&
      ownedQuantity_ == owned) {
    return;
  }
  order_ = order;
  item_ = item;
  quote_ = quote;
  ownedQuantity_ = owned;
  const bool visible = order.value("visible").toBool();
  const bool selling = order.value("type").toString() == "sell";
  const int quantity = order.value("quantity").toInt(1);
  const bool warning = selling && owned >= 0 && owned < quantity;
  {
    const QSignalBlocker blocker(visibility_);
    visibility_->setChecked(visible);
  }
  visibility_->setToolTip(visible ? "Public order" : "Hidden order");
  name_->setText(item.value("name").toString(item.isEmpty() ? "Loading item..."
                                                            : "Unknown item"));
  owned_->setText(owned >= 0 ? QString("%1 owned").arg(owned) : "-- owned");
  warning_->setVisible(warning);
  top_->layout()->setContentsMargins(0, 0, warning ? 0 : 7, 0);
  if (body_->property("private").toBool() != !visible ||
      !body_->property("private").isValid()) {
    body_->setProperty("private", !visible);
    body_->style()->unpolish(body_);
    body_->style()->polish(body_);
  }
  opacity_->setEnabled(!visible);
  const QString sideName = selling ? "marketSellBadge" : "marketBuyBadge";
  if (side_->objectName() != sideName) {
    side_->setObjectName(sideName);
    side_->style()->unpolish(side_);
    side_->style()->polish(side_);
  }
  side_->setText(selling ? "WTS" : "WTB");
  quantity_->setText(QString::number(quantity));
  price_->setText(QString::number(order.value("platinum").toInt()));
  extra_->setText(extraOrderData(order));
  comparisonTitle_->setText(selling ? "Lowest price:" : "Highest price:");
  const QJsonValue comparison = quote.value("quote").toObject().value(
      selling ? "lowest_sell" : "highest_buy");
  comparison_->setText(
      comparison.isDouble() ? QString::number(comparison.toInt()) : "-");
}
