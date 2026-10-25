#include "market_widget.h"

#include <QButtonGroup>
#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QEasingCurve>
#include <QEvent>
#include <QFormLayout>
#include <QFrame>
#include <QGraphicsOpacityEffect>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QHideEvent>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QMessageBox>
#include <QPropertyAnimation>
#include <QPushButton>
#include <QScrollArea>
#include <QShowEvent>
#include <QSignalBlocker>
#include <QSpinBox>
#include <QStackedWidget>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>

#include <algorithm>
#include <optional>

#include "animated_progress_bar.h"
#include "app_controller.h"
#include "market_order_card.h"
#include "market_spin_box.h"
#include "widget_capture.h"

namespace {
QString firstString(const QJsonObject &object,
                    std::initializer_list<const char *> keys) {
  for (const char *key : keys) {
    const QString value = object.value(key).toString();
    if (!value.isEmpty()) {
      return value;
    }
  }
  return {};
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

QJsonObject orderFilters(const QJsonObject &order) {
  QJsonObject filters;
  for (const char *key : {"rank", "charges", "amberStars", "cyanStars"}) {
    if (order.value(key).isDouble()) {
      filters.insert(key, order.value(key));
    }
  }
  if (!order.value("subtype").toString().isEmpty()) {
    filters.insert("subtype", order.value("subtype"));
  }
  return filters;
}

std::optional<QJsonObject> editOrderDialog(QWidget *parent,
                                           const QJsonObject &order,
                                           const QJsonObject &item) {
  QDialog dialog(parent);
  dialog.setWindowTitle("Edit Market order");
  auto *layout = new QVBoxLayout(&dialog);
  auto *form = new QFormLayout;
  auto *quantity = new wfgui::MarketSpinBox;
  quantity->setRange(1, 9999);
  quantity->setValue(order.value("quantity").toInt(1));
  form->addRow("Quantity", quantity);
  auto *price = new wfgui::MarketSpinBox;
  price->setRange(1, 900000);
  price->setValue(order.value("platinum").toInt(1));
  form->addRow("Platinum", price);
  auto *visible = new QCheckBox("Public");
  visible->setChecked(order.value("visible").toBool());
  form->addRow("Visibility", visible);
  QSpinBox *rank = nullptr;
  if (item.value("max_rank").toInt() > 0 || order.value("rank").isDouble()) {
    rank = new wfgui::MarketSpinBox;
    rank->setRange(
        0, qMax(item.value("max_rank").toInt(), order.value("rank").toInt()));
    rank->setValue(order.value("rank").toInt());
    form->addRow("Rank", rank);
  }
  auto addOptionalSpin = [form, &order, &item](const char *field,
                                               const char *maximum,
                                               const QString &label) {
    QSpinBox *spin = nullptr;
    if (item.value(maximum).toInt() > 0 || order.value(field).isDouble()) {
      spin = new wfgui::MarketSpinBox;
      spin->setRange(
          0, qMax(item.value(maximum).toInt(), order.value(field).toInt()));
      spin->setValue(order.value(field).toInt());
      form->addRow(label, spin);
    }
    return spin;
  };
  QSpinBox *charges = addOptionalSpin("charges", "max_charges", "Charges");
  QSpinBox *amberStars =
      addOptionalSpin("amberStars", "max_amber_stars", "Amber stars");
  QSpinBox *cyanStars =
      addOptionalSpin("cyanStars", "max_cyan_stars", "Cyan stars");
  QSpinBox *perTrade = nullptr;
  if (item.value("bulk_tradable").toBool() ||
      firstInt(order, {"perTrade", "per_trade"}, 1) > 1) {
    perTrade = new wfgui::MarketSpinBox;
    perTrade->setRange(1, 6);
    perTrade->setValue(firstInt(order, {"perTrade", "per_trade"}, 1));
    quantity->setSingleStep(perTrade->value());
    QObject::connect(
        perTrade, &QSpinBox::valueChanged, quantity, [quantity](int step) {
          quantity->setSingleStep(step);
          quantity->setValue(qMax(step, quantity->value() / step * step));
        });
    form->addRow("Per trade", perTrade);
  }
  QComboBox *subtype = nullptr;
  const QJsonArray subtypes = item.value("subtypes").toArray();
  if (!subtypes.isEmpty() || !order.value("subtype").toString().isEmpty()) {
    subtype = new QComboBox;
    for (const QJsonValue &value : subtypes) {
      subtype->addItem(value.toString(), value.toString());
    }
    if (subtype->findData(order.value("subtype").toString()) < 0) {
      subtype->addItem(order.value("subtype").toString(),
                       order.value("subtype").toString());
    }
    subtype->setCurrentIndex(
        subtype->findData(order.value("subtype").toString()));
    form->addRow("Type", subtype);
  }
  layout->addLayout(form);
  auto *buttons =
      new QDialogButtonBox(QDialogButtonBox::Save | QDialogButtonBox::Cancel);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog,
                   &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog,
                   &QDialog::reject);
  layout->addWidget(buttons);
  if (dialog.exec() != QDialog::Accepted) {
    return std::nullopt;
  }
  QJsonObject patch{{"quantity", quantity->value()},
                    {"platinum", price->value()},
                    {"visible", visible->isChecked()}};
  if (rank) {
    patch.insert("rank", rank->value());
  }
  if (charges) {
    patch.insert("charges", charges->value());
  }
  if (amberStars) {
    patch.insert("amberStars", amberStars->value());
  }
  if (cyanStars) {
    patch.insert("cyanStars", cyanStars->value());
  }
  if (perTrade) {
    const int step = perTrade->value();
    const int normalized = qMax(step, quantity->value() / step * step);
    patch.insert("quantity", normalized);
    patch.insert("perTrade", step);
  }
  if (subtype && subtype->currentIndex() >= 0) {
    patch.insert("subtype", subtype->currentData().toString());
  }
  return patch;
}
} // namespace

MarketWidget::MarketWidget(AppController *controller, QWidget *parent)
    : QWidget(parent), controller_(controller), views_(new QStackedWidget),
      loadingView_(new QWidget), loginView_(new QWidget),
      ordersView_(new QWidget), email_(new QLineEdit), password_(new QLineEdit),
      loginError_(new QLabel), login_(new QPushButton("Sign in")),
      accountName_(new QLabel), presence_(new QComboBox),
      presenceState_(new QLabel), summary_(new QLabel),
      progress_(new AnimatedProgressBar), categories_(new QButtonGroup(this)),
      search_(new QLineEdit), side_(new QComboBox), visibility_(new QComboBox),
      inventory_(new QComboBox), sort_(new QComboBox),
      direction_(new QToolButton), scroll_(new QScrollArea),
      orderHost_(new QWidget), orderGrid_(new QGridLayout(orderHost_)),
      refreshTimer_(new QTimer(this)), rebuildTimer_(new QTimer(this)) {
  setObjectName("page");
  views_->setObjectName("marketViews");
  loadingView_->setObjectName("marketView");
  loginView_->setObjectName("marketView");
  ordersView_->setObjectName("marketView");
  orderHost_->setObjectName("marketOrderHost");
  wfgui::setCaptureTarget(orderHost_, "market.orders", true);
  auto *pageLayout = new QVBoxLayout(this);
  pageLayout->setContentsMargins(18, 18, 18, 0);
  pageLayout->setSpacing(10);

  auto *header = new QHBoxLayout;
  auto *title = new QLabel("Market");
  title->setObjectName("pageTitle");
  header->addWidget(title);
  header->addStretch();
  accountName_->setObjectName("secondaryText");
  header->addWidget(accountName_);
  presence_->setObjectName("marketPresence");
  presence_->addItem("Auto", "auto");
  presence_->addItem("In game", "ingame");
  presence_->addItem("Online", "online");
  presence_->addItem("Offline", "invisible");
  presence_->setToolTip("Warframe.market presence");
  header->addWidget(presence_);
  presenceState_->setObjectName("marketPresenceState");
  header->addWidget(presenceState_);
  auto *refresh = new QPushButton("Refresh");
  header->addWidget(refresh);
  auto *logout = new QPushButton("Log out");
  logout->setObjectName("marketLogout");
  header->addWidget(logout);
  pageLayout->addLayout(header);

  progress_->setObjectName("priceProgress");
  progress_->setTextVisible(false);
  progress_->setRange(0, 1);
  progress_->setValue(0);
  pageLayout->addWidget(progress_);
  pageLayout->addWidget(views_, 1);

  auto *loadingLayout = new QVBoxLayout(loadingView_);
  loadingLayout->addStretch();
  auto *loading = new AnimatedProgressBar;
  loading->setRange(0, 0);
  loading->setMaximumWidth(240);
  loadingLayout->addWidget(loading, 0, Qt::AlignHCenter);
  loadingLayout->addStretch();

  auto *loginLayout = new QVBoxLayout(loginView_);
  loginLayout->addStretch();
  auto *loginPanel = new QWidget;
  loginPanel->setObjectName("marketLoginPanel");
  loginPanel->setMaximumWidth(440);
  auto *loginPanelLayout = new QVBoxLayout(loginPanel);
  loginPanelLayout->setContentsMargins(24, 22, 24, 22);
  loginPanelLayout->setSpacing(10);
  auto *loginTitle = new QLabel("Warframe.market");
  loginTitle->setObjectName("marketLoginTitle");
  loginPanelLayout->addWidget(loginTitle);
  auto *loginText = new QLabel(
      "Sign in to manage regular item orders. Credentials are sent once to "
      "Warframe.market; only the session token is stored locally.");
  loginText->setObjectName("secondaryText");
  loginText->setWordWrap(true);
  loginPanelLayout->addWidget(loginText);
  email_->setPlaceholderText("Email");
  email_->setClearButtonEnabled(true);
  password_->setPlaceholderText("Password");
  password_->setEchoMode(QLineEdit::Password);
  loginPanelLayout->addWidget(email_);
  loginPanelLayout->addWidget(password_);
  loginError_->setObjectName("marketError");
  loginError_->setWordWrap(true);
  loginPanelLayout->addWidget(loginError_);
  login_->setObjectName("marketPrimary");
  loginPanelLayout->addWidget(login_);
  loginLayout->addWidget(loginPanel, 0, Qt::AlignHCenter);
  loginLayout->addStretch();

  auto *ordersLayout = new QVBoxLayout(ordersView_);
  ordersLayout->setContentsMargins(0, 0, 0, 0);
  ordersLayout->setSpacing(10);
  auto *tabs = new QHBoxLayout;
  auto *myOrders = new QLabel("My orders");
  myOrders->setObjectName("marketTabTitle");
  tabs->addWidget(myOrders);
  tabs->addStretch();
  summary_->setObjectName("secondaryText");
  tabs->addWidget(summary_);
  ordersLayout->addLayout(tabs);

  auto *categoryRow = new QHBoxLayout;
  categoryRow->setSpacing(6);
  const QStringList categories = {"All",     "Parts", "Relics", "Mods",
                                  "Arcanes", "Misc",  "Sets"};
  for (int index = 0; index < categories.size(); ++index) {
    auto *button = new QPushButton(categories.at(index));
    button->setObjectName("filterChip");
    button->setCheckable(true);
    button->setChecked(index == 0);
    button->setProperty("category", categories.at(index).toLower());
    categories_->addButton(button, index);
    categoryRow->addWidget(button);
  }
  categoryRow->addStretch();
  ordersLayout->addLayout(categoryRow);

  auto *controls = new QHBoxLayout;
  search_->setPlaceholderText("Filter orders");
  search_->setClearButtonEnabled(true);
  controls->addWidget(search_, 1);
  side_->addItem("All orders", "all");
  side_->addItem("WTS", "sell");
  side_->addItem("WTB", "buy");
  side_->setObjectName("marketFilter");
  controls->addWidget(side_);
  visibility_->addItem("Public and hidden", "all");
  visibility_->addItem("Public", "public");
  visibility_->addItem("Hidden", "hidden");
  visibility_->setObjectName("marketFilter");
  controls->addWidget(visibility_);
  inventory_->addItem("Any inventory", "all");
  inventory_->addItem("Available", "available");
  inventory_->addItem("Missing", "missing");
  inventory_->setObjectName("marketFilter");
  controls->addWidget(inventory_);
  direction_->setObjectName("sortDirection");
  direction_->setText("↑");
  direction_->setToolTip("Reverse order");
  controls->addWidget(direction_);
  sort_->setObjectName("sortSelect");
  sort_->addItem("Name", "name");
  sort_->addItem("Platinum", "platinum");
  sort_->addItem("Amount", "quantity");
  controls->addWidget(sort_);
  auto *actions = new QToolButton;
  actions->setObjectName("marketActions");
  actions->setText("Actions");
  actions->setPopupMode(QToolButton::InstantPopup);
  auto *actionsMenu = new QMenu(actions);
  actionsMenu->addAction("Set all orders public", this,
                         [this] { controller_->setMarketOrdersVisible(true); });
  actionsMenu->addAction("Set all orders hidden", this, [this] {
    controller_->setMarketOrdersVisible(false);
  });
  actionsMenu->addSeparator();
  actionsMenu->addAction("Set WTS public", this, [this] {
    controller_->setMarketOrdersVisible(true, "sell");
  });
  actionsMenu->addAction("Set WTS hidden", this, [this] {
    controller_->setMarketOrdersVisible(false, "sell");
  });
  actionsMenu->addAction("Set WTB public", this, [this] {
    controller_->setMarketOrdersVisible(true, "buy");
  });
  actionsMenu->addAction("Set WTB hidden", this, [this] {
    controller_->setMarketOrdersVisible(false, "buy");
  });
  actionsMenu->addSeparator();
  actionsMenu->addAction("Reconcile WTS quantities...", this,
                         &MarketWidget::reconcileSellOrders);
  actionsMenu->addAction("Delete filtered orders...", this,
                         &MarketWidget::deleteFiltered);
  actions->setMenu(actionsMenu);
  controls->addWidget(actions);
  ordersLayout->addLayout(controls);

  scroll_->setObjectName("marketScroll");
  scroll_->setWidgetResizable(true);
  scroll_->setFrameShape(QFrame::NoFrame);
  orderGrid_->setContentsMargins(0, 4, 0, 10);
  orderGrid_->setHorizontalSpacing(10);
  orderGrid_->setVerticalSpacing(10);
  orderGrid_->setAlignment(Qt::AlignTop);
  scroll_->setWidget(orderHost_);
  scroll_->viewport()->installEventFilter(this);
  ordersLayout->addWidget(scroll_, 1);

  views_->addWidget(loadingView_);
  views_->addWidget(loginView_);
  views_->addWidget(ordersView_);

  connect(refresh, &QPushButton::clicked, this, [this] {
    controller_->refreshMarket();
    refreshQuotes(true);
  });
  connect(presence_, &QComboBox::activated, this, [this](int index) {
    controller_->setMarketPresenceMode(presence_->itemData(index).toString());
  });
  connect(controller_, &AppController::marketVariantQuoteReady, this,
          &MarketWidget::scheduleRebuild);
  connect(logout, &QPushButton::clicked, controller_,
          &AppController::marketLogout);
  connect(login_, &QPushButton::clicked, this, [this] {
    if (email_->text().trimmed().isEmpty() || password_->text().isEmpty()) {
      loginError_->setText("Enter email and password.");
      return;
    }
    controller_->marketLogin(email_->text().trimmed(), password_->text());
    password_->clear();
  });
  connect(password_, &QLineEdit::returnPressed, login_, &QPushButton::click);
  connect(categories_, &QButtonGroup::idClicked, this, [this](int id) {
    category_ = categories_->button(id)->property("category").toString();
    rebuildOrders();
  });
  connect(search_, &QLineEdit::textChanged, this, &MarketWidget::rebuildOrders);
  connect(side_, &QComboBox::currentIndexChanged, this,
          &MarketWidget::rebuildOrders);
  connect(visibility_, &QComboBox::currentIndexChanged, this,
          &MarketWidget::rebuildOrders);
  connect(inventory_, &QComboBox::currentIndexChanged, this,
          &MarketWidget::rebuildOrders);
  connect(sort_, &QComboBox::currentIndexChanged, this,
          &MarketWidget::rebuildOrders);
  connect(direction_, &QToolButton::clicked, this, [this] {
    descending_ = !descending_;
    direction_->setText(descending_ ? "↓" : "↑");
    rebuildOrders();
  });
  connect(controller_, &AppController::marketAccountChanged, this,
          &MarketWidget::updateState);
  connect(controller_, &AppController::marketCatalogChanged, this,
          &MarketWidget::scheduleRebuild);
  connect(controller_, &AppController::marketQuotesChanged, this,
          &MarketWidget::scheduleRebuild);
  connect(controller_, &AppController::inventoryStateChanged, this,
          &MarketWidget::scheduleRebuild);
  connect(controller_, &AppController::assetsChanged, this,
          [this](const QStringList &ids) {
            for (auto card = cardsById_.cbegin(); card != cardsById_.cend();
                 ++card) {
              const QString asset = cardAssets_.value(card.key());
              if (ids.contains(asset)) {
                card.value()->setAsset(controller_->assetRef(asset));
              }
            }
          });
  rebuildTimer_->setSingleShot(true);
  rebuildTimer_->setInterval(16);
  connect(rebuildTimer_, &QTimer::timeout, this, &MarketWidget::rebuildOrders);
  refreshTimer_->setInterval(60'000);
  connect(refreshTimer_, &QTimer::timeout, this, [this] {
    if (controller_->marketAccount().value("authenticated").toBool() &&
        !controller_->marketBusy()) {
      controller_->refreshMarket();
      refreshQuotes(false);
    }
  });
  updateState();
}

bool MarketWidget::eventFilter(QObject *watched, QEvent *event) {
  if (watched == scroll_->viewport() && event->type() == QEvent::Resize) {
    relayoutOrders();
  }
  return QWidget::eventFilter(watched, event);
}

void MarketWidget::showEvent(QShowEvent *event) {
  QWidget::showEvent(event);
  refreshTimer_->start();
  if (rebuildPending_) {
    scheduleRebuild();
  }
  controller_->ensureInventory();
  controller_->ensureMarket();
  refreshQuotes(false);
}

void MarketWidget::hideEvent(QHideEvent *event) {
  refreshTimer_->stop();
  QWidget::hideEvent(event);
}

void MarketWidget::updateState() {
  const QJsonObject account = controller_->marketAccount();
  const bool authenticated = account.value("authenticated").toBool();
  presence_->setVisible(authenticated);
  presenceState_->setVisible(false);
  if (auto *logout = findChild<QPushButton *>("marketLogout")) {
    logout->setVisible(authenticated);
  }
  const bool initialLoading = !controller_->marketLoaded();
  progress_->setRange(0, controller_->marketBusy() ? 0 : 1);
  if (!controller_->marketBusy()) {
    progress_->setValue(0);
  }
  login_->setEnabled(!controller_->marketBusy());
  loginError_->setText(controller_->marketError());
  if (initialLoading) {
    setView(loadingView_);
    return;
  }
  if (!authenticated) {
    accountName_->clear();
    setView(loginView_);
    return;
  }
  const QJsonObject profile = account.value("profile").toObject();
  accountName_->setText(firstString(profile, {"ingameName", "ingame_name"}));
  const QJsonObject presence = account.value("presence").toObject();
  const QString mode = presence.value("mode").toString("invisible");
  {
    const QSignalBlocker blocker(presence_);
    const int index = presence_->findData(mode);
    presence_->setCurrentIndex(index >= 0 ? index
                                          : presence_->findData("invisible"));
  }
  const QString status = presence.value("status").toString();
  const QString presenceError = presence.value("error").toString();
  QString state = status;
  if (!presenceError.isEmpty()) {
    state = "Connection error";
  } else if (presence.value("connecting").toBool()) {
    state = "Connecting";
  } else if (state == "ingame") {
    state = "In game";
  } else if (!state.isEmpty()) {
    state = state.left(1).toUpper() + state.mid(1);
  }
  presenceState_->setText(state);
  presenceState_->setToolTip(presenceError);
  presenceState_->setVisible(
      authenticated &&
      (!presenceError.isEmpty() || presence.value("connecting").toBool() ||
       mode == "auto" || (!status.isEmpty() && status != mode)));
  setView(ordersView_);
  rebuildOrders();
}

void MarketWidget::scheduleRebuild() {
  if (!isVisible()) {
    rebuildPending_ = true;
    return;
  }
  rebuildTimer_->start();
}

void MarketWidget::rebuildOrders() {
  if (!isVisible()) {
    rebuildPending_ = true;
    return;
  }
  rebuildPending_ = false;
  if (!controller_->marketAccount().value("authenticated").toBool()) {
    return;
  }
  const QJsonArray orders =
      controller_->marketAccount().value("orders").toArray();
  filteredOrders_.clear();
  const QString search = search_->text().trimmed();
  const QString side = side_->currentData().toString();
  const QString visibility = visibility_->currentData().toString();
  const QString inventory = inventory_->currentData().toString();
  for (const QJsonValue &value : orders) {
    const QJsonObject order = value.toObject();
    const QString name = orderName(order);
    const bool visible = order.value("visible").toBool();
    const bool missing = orderMissing(order);
    if (!search.isEmpty() && !name.contains(search, Qt::CaseInsensitive)) {
      continue;
    }
    if (side != "all" && order.value("type").toString() != side) {
      continue;
    }
    if ((visibility == "public" && !visible) ||
        (visibility == "hidden" && visible)) {
      continue;
    }
    if ((inventory == "missing" && !missing) ||
        (inventory == "available" && missing)) {
      continue;
    }
    if (category_ != "all" && orderCategory(order) != category_) {
      continue;
    }
    filteredOrders_.append(order);
  }
  const QString sort = sort_->currentData().toString();
  std::sort(filteredOrders_.begin(), filteredOrders_.end(),
            [this, &sort](const QJsonObject &left, const QJsonObject &right) {
              int comparison = 0;
              if (sort == "platinum") {
                comparison = left.value("platinum").toInt() -
                             right.value("platinum").toInt();
              } else if (sort == "quantity") {
                comparison = left.value("quantity").toInt() -
                             right.value("quantity").toInt();
              } else {
                comparison = QString::localeAwareCompare(orderName(left),
                                                         orderName(right));
              }
              if (comparison == 0) {
                comparison = QString::compare(left.value("id").toString(),
                                              right.value("id").toString());
              }
              return descending_ ? comparison > 0 : comparison < 0;
            });

  QList<QWidget *> cards;
  QSet<QString> kept;
  qint64 sellTotal = 0;
  qint64 buyTotal = 0;
  for (const QJsonObject &order : std::as_const(filteredOrders_)) {
    QJsonObject item =
        controller_->marketItem(order.value("itemId").toString());
    const QString assetId =
        item.value("asset").toObject().value("id").toString();
    const QString name = item.value("name").toString(
        item.isEmpty() ? "Loading item..." : "Unknown item");
    const QJsonObject filters = orderFilters(order);
    const int owned =
        item.isEmpty() ? -1 : controller_->ownedMarketQuantity(name, filters);
    const QString itemId = order.value("itemId").toString();
    QJsonObject quote = filters.isEmpty()
                            ? controller_->marketQuote(itemId)
                            : controller_->marketVariantQuote(itemId, filters);
    if (!filters.isEmpty() && quote.isEmpty()) {
      controller_->requestMarketVariantQuote(itemId, filters);
    }
    const QString id = order.value("id").toString();
    const int quantity = order.value("quantity").toInt(1);
    const int perTrade = firstInt(order, {"perTrade", "per_trade"}, 1);
    const qint64 total = static_cast<qint64>(order.value("platinum").toInt()) *
                         qMax(1, quantity / perTrade);
    if (order.value("type").toString() == "sell") {
      sellTotal += total;
    } else {
      buyTotal += total;
    }
    MarketOrderCardActions actions{
        .visibility =
            [this, id, order] {
              controller_->marketUpdateOrder(
                  id, {{"visible", !order.value("visible").toBool()}});
            },
        .edit = [this, order] { editOrder(order); },
        .add =
            [this, id, quantity, perTrade] {
              controller_->marketUpdateOrder(
                  id, {{"quantity", quantity + perTrade}});
            },
        .close = [this, id,
                  perTrade] { controller_->marketCloseOrder(id, perTrade); },
        .remove =
            [this, id, name] {
              if (QMessageBox::question(
                      this, "Delete Market order",
                      QString("Delete order for %1?").arg(name)) ==
                  QMessageBox::Yes) {
                controller_->marketDeleteOrder(id);
              }
            },
        .listings =
            [this, itemId, order] {
              emit marketItemRequested(itemId, order.value("type").toString());
            }};
    MarketOrderCard *card = cardsById_.value(id);
    if (card) {
      card->updateOrder(order, item, quote, owned, std::move(actions));
    } else {
      card = new MarketOrderCard(order, item, quote, owned, std::move(actions),
                                 orderHost_);
      cardsById_.insert(id, card);
    }
    card->setAsset(controller_->assetRef(assetId));
    cardAssets_.insert(id, assetId);
    kept.insert(id);
    cards.append(card);
  }
  for (auto card = cardsById_.begin(); card != cardsById_.end();) {
    if (kept.contains(card.key())) {
      ++card;
    } else {
      cardAssets_.remove(card.key());
      delete card.value();
      card = cardsById_.erase(card);
    }
  }
  summary_->setText(
      QString("WTS: %1  ·  WTB: %2").arg(sellTotal).arg(buyTotal));
  if (filteredOrders_.isEmpty()) {
    if (!empty_) {
      empty_ = new QLabel("No Market orders match these filters.", orderHost_);
      empty_->setObjectName("emptyState");
      empty_->setAlignment(Qt::AlignCenter);
    }
    cards.append(empty_);
  }
  if (empty_) {
    empty_->setVisible(filteredOrders_.isEmpty());
  }
  const bool changed = orderCards_ != cards;
  orderCards_ = std::move(cards);
  relayoutOrders(changed);
}

void MarketWidget::refreshQuotes(bool force) {
  QStringList items;
  for (const QJsonObject &order : std::as_const(filteredOrders_)) {
    const QString item = order.value("itemId").toString();
    const QJsonObject filters = orderFilters(order);
    if (filters.isEmpty()) {
      items.append(item);
    } else {
      controller_->requestMarketVariantQuote(item, filters, force);
    }
  }
  items.removeDuplicates();
  controller_->resolveMarketQuotes(items, force);
}

void MarketWidget::relayoutOrders(bool force) {
  if (orderCards_.isEmpty()) {
    return;
  }
  const int available = qMax(1, scroll_->viewport()->width());
  const int columns = qMax(1, available / 380);
  if (!force && columns_ == columns &&
      orderGrid_->count() == orderCards_.size()) {
    return;
  }
  while (QLayoutItem *item = orderGrid_->takeAt(0)) {
    delete item;
  }
  for (int column = 0; column < qMax(columns_, columns); ++column) {
    orderGrid_->setColumnStretch(column, column < columns ? 1 : 0);
  }
  for (int index = 0; index < orderCards_.size(); ++index) {
    orderGrid_->addWidget(orderCards_.at(index), index / columns,
                          index % columns);
  }
  columns_ = columns;
  orderGrid_->activate();
}

void MarketWidget::setView(QWidget *view) {
  if (views_->currentWidget() == view) {
    return;
  }
  views_->setCurrentWidget(view);
  auto *effect = new QGraphicsOpacityEffect(view);
  view->setGraphicsEffect(effect);
  auto *animation = new QPropertyAnimation(effect, "opacity", view);
  animation->setDuration(160);
  animation->setStartValue(0.0);
  animation->setEndValue(1.0);
  animation->setEasingCurve(QEasingCurve::OutCubic);
  connect(animation, &QPropertyAnimation::finished, view,
          [view] { view->setGraphicsEffect(nullptr); });
  animation->start(QAbstractAnimation::DeleteWhenStopped);
}

void MarketWidget::editOrder(const QJsonObject &order) {
  const QJsonObject item =
      controller_->marketItem(order.value("itemId").toString());
  const std::optional<QJsonObject> patch = editOrderDialog(this, order, item);
  if (patch.has_value()) {
    controller_->marketUpdateOrder(order.value("id").toString(), *patch);
  }
}

void MarketWidget::deleteFiltered() {
  if (filteredOrders_.isEmpty() ||
      QMessageBox::warning(
          this, "Delete Market orders",
          QString("Delete %1 filtered order(s)? This cannot be undone.")
              .arg(filteredOrders_.size()),
          QMessageBox::Yes | QMessageBox::Cancel,
          QMessageBox::Cancel) != QMessageBox::Yes) {
    return;
  }
  for (const QJsonObject &order : std::as_const(filteredOrders_)) {
    controller_->marketDeleteOrder(order.value("id").toString());
  }
}

void MarketWidget::reconcileSellOrders() {
  QList<QJsonObject> changes;
  QStringList unresolved;
  bool inventoryPending = false;
  for (const QJsonValue &value :
       controller_->marketAccount().value("orders").toArray()) {
    const QJsonObject order = value.toObject();
    if (order.value("type").toString() != "sell") {
      continue;
    }
    const QString itemId = order.value("itemId").toString();
    const QJsonObject item = controller_->marketItem(itemId);
    if (item.isEmpty()) {
      unresolved.append(itemId);
      continue;
    }
    const QString name = item.value("name").toString();
    const int owned = controller_->ownedMarketQuantity(name, orderFilters(order));
    if (owned < 0) {
      inventoryPending = true;
      continue;
    }
    const int perTrade = firstInt(order, {"perTrade", "per_trade"}, 1);
    const int target = owned / perTrade * perTrade;
    if (target < order.value("quantity").toInt()) {
      changes.append(order);
    }
  }
  if (!unresolved.isEmpty()) {
    controller_->describeMarketItems(unresolved);
    QMessageBox::information(this, "Market orders",
                             "Market item metadata is still loading.");
    return;
  }
  if (inventoryPending) {
    QMessageBox::information(this, "Market orders",
                             "Player inventory is still loading.");
    return;
  }
  if (changes.isEmpty()) {
    QMessageBox::information(this, "Market orders",
                             "Sell order quantities match inventory.");
    return;
  }
  if (QMessageBox::question(
          this, "Reconcile Market orders",
          QString("Adjust or remove %1 sell order(s) to match inventory?")
              .arg(changes.size())) != QMessageBox::Yes) {
    return;
  }
  for (const QJsonObject &order : std::as_const(changes)) {
    const int owned =
        controller_->ownedMarketQuantity(orderName(order), orderFilters(order));
    const int perTrade = firstInt(order, {"perTrade", "per_trade"}, 1);
    const int target = owned / perTrade * perTrade;
    const QString id = order.value("id").toString();
    if (target < perTrade) {
      controller_->marketDeleteOrder(id);
    } else {
      controller_->marketUpdateOrder(id, {{"quantity", target}});
    }
  }
}

QString MarketWidget::orderName(const QJsonObject &order) const {
  const QJsonObject item =
      controller_->marketItem(order.value("itemId").toString());
  return item.value("name").toString(item.isEmpty() ? "Loading item..."
                                                    : "Unknown item");
}

QString MarketWidget::orderCategory(const QJsonObject &order) const {
  const QJsonObject item =
      controller_->marketItem(order.value("itemId").toString());
  const QString name = item.value("name").toString().toLower();
  QStringList tags;
  for (const QJsonValue &value : item.value("tags").toArray()) {
    tags.append(value.toString().toLower());
  }
  if (tags.contains("relic") || name.contains(" relic")) {
    return "relics";
  }
  if (tags.contains("mod") || name.contains(" mod")) {
    return "mods";
  }
  if (tags.contains("arcane") || name.startsWith("arcane ")) {
    return "arcanes";
  }
  if (tags.contains("set") || name.endsWith(" set")) {
    return "sets";
  }
  if (name.contains(" blueprint") || name.contains(" chassis") ||
      name.contains(" neuroptics") || name.contains(" systems") ||
      name.contains(" barrel") || name.contains(" receiver") ||
      name.contains(" blade") || name.contains(" handle")) {
    return "parts";
  }
  return "misc";
}

bool MarketWidget::orderMissing(const QJsonObject &order) const {
  if (order.value("type").toString() != "sell") {
    return false;
  }
  if (controller_->marketItem(order.value("itemId").toString()).isEmpty()) {
    return false;
  }
  const int owned =
      controller_->ownedMarketQuantity(orderName(order), orderFilters(order));
  return owned >= 0 && owned < order.value("quantity").toInt();
}
