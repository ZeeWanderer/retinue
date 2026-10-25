#include "inventory_widget.h"

#include <QAbstractItemModel>
#include <QActionGroup>
#include <QButtonGroup>
#include <QComboBox>
#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QPushButton>
#include <QStackedLayout>
#include <QStyle>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>

#include <algorithm>
#include <array>
#include <utility>

#include "animated_progress_bar.h"
#include "app_controller.h"
#include "compact_search.h"
#include "player_item_grid_widget.h"
#include "player_item_model.h"

namespace {
struct Filter {
  const char *label;
  const char *value;
  const char *icon;
};

constexpr std::array<Filter, 6> Groups{{
    {"All parts", "parts", ":/resources/categories/all.png"},
    {"Relics", "relics", ":/resources/categories/relics.png"},
    {"Mods", "mods", ":/resources/categories/mods.png"},
    {"Arcanes", "arcanes", ":/resources/categories/arcanes.png"},
    {"Misc", "misc", ":/resources/categories/misc.png"},
    {"Sets", "sets", ":/resources/categories/sets.png"},
}};

void updateFilterButtons(QButtonGroup *group) {
  for (auto *button : group->buttons()) {
    const QString label = button->property("label").toString();
    button->setText(button->isChecked() || button->icon().isNull() ? label
                                                                   : QString());
  }
}

void updateActiveFilterStyle(QToolButton *button,
                             const QList<QActionGroup *> &groups) {
  const bool active = std::any_of(
      groups.cbegin(), groups.cend(), [](const QActionGroup *group) {
        return group->checkedAction() &&
               group->checkedAction()->data().toInt() >= 0;
      });
  button->setProperty("active", active);
  button->style()->unpolish(button);
  button->style()->polish(button);
}
} // namespace

InventoryWidget::InventoryWidget(AppController *controller, QWidget *parent)
    : QWidget(parent), controller_(controller),
      items_(new PlayerItemFilterModel(this)),
      grid_(new PlayerItemGridWidget(PlayerItemGridWidget::Kind::Inventory)),
      emptyState_(new QLabel), progress_(new AnimatedProgressBar),
      refresh_(new QPushButton), content_(new QStackedLayout),
      priceUpdateTimer_(new QTimer(this)) {
  setObjectName("page");
  items_->setSourceModel(controller_->inventoryItems());
  items_->setGroup("parts");
  grid_->setModel(items_);

  auto *layout = new QVBoxLayout(this);
  layout->setContentsMargins(10, 10, 10, 0);
  layout->setSpacing(8);

  auto *toolbar = new QHBoxLayout;
  toolbar->setSpacing(6);
  auto *groupButtons = new QButtonGroup(this);
  groupButtons->setExclusive(true);
  int id = 0;
  for (const auto &[label, value, icon] : Groups) {
    auto *button = new QPushButton;
    button->setObjectName("filterChip");
    button->setCheckable(true);
    button->setProperty("group", value);
    button->setProperty("label", label);
    button->setIcon(QIcon(icon));
    button->setIconSize({22, 22});
    button->setToolTip(label);
    button->setChecked(id == 0);
    groupButtons->addButton(button, id++);
    toolbar->addWidget(button);
  }
  toolbar->addStretch();
  auto *compactSearch = new CompactSearch("Search inventory");
  auto *search = compactSearch->editor();
  toolbar->addWidget(compactSearch);

  auto *filter = new QToolButton;
  filter->setObjectName("compactTool");
  filter->setIcon(QIcon(":/resources/ui/filter.png"));
  filter->setIconSize({17, 17});
  filter->setToolTip("Filter inventory");
  filter->setPopupMode(QToolButton::InstantPopup);
  auto *filterMenu = new QMenu(filter);
  filter->setMenu(filterMenu);
  struct BooleanFilter {
    const char *title;
    const char *key;
    const char *yes;
    const char *no;
  };
  constexpr std::array<BooleanFilter, 5> booleanFilters{{
      {"Type", "prime", "Prime", "Normal"},
      {"Mastery", "mastered", "Mastered", "Unmastered"},
      {"Copies", "duplicate", "More than one", "One"},
      {"Vaulted", "vaulted", "Vaulted", "Unvaulted"},
      {"Set completion", "complete", "Complete", "Incomplete"},
  }};
  QList<QActionGroup *> filterGroups;
  QList<QString> filterKeys;
  for (const auto &[title, key, yes, no] : booleanFilters) {
    auto *submenu = filterMenu->addMenu(title);
    auto *group = new QActionGroup(filterMenu);
    group->setExclusive(true);
    for (const auto &[label, state] :
         std::array<std::pair<const char *, int>, 3>{
             {{"Any", -1}, {yes, 1}, {no, 0}}}) {
      auto *action = submenu->addAction(label);
      action->setCheckable(true);
      action->setChecked(state == -1);
      action->setData(state);
      group->addAction(action);
    }
    filterGroups.append(group);
    filterKeys.append(key);
  }
  for (int index = 0; index < filterGroups.size(); ++index) {
    QActionGroup *group = filterGroups.at(index);
    const QString key = filterKeys.at(index);
    connect(group, &QActionGroup::triggered, this,
            [this, key, group, filter, filterGroups](QAction *) {
              items_->setFlag(key, group->checkedAction()->data().toInt());
              updateActiveFilterStyle(filter, filterGroups);
              if (priceSort()) {
                beginPriceLoad();
              } else {
                updateContent();
              }
            });
  }
  toolbar->addWidget(filter);

  auto *sortControl = new QWidget;
  sortControl->setObjectName("sortControl");
  auto *sortLayout = new QHBoxLayout(sortControl);
  sortLayout->setContentsMargins(0, 0, 0, 0);
  sortLayout->setSpacing(0);
  auto *sortDirection = new QToolButton;
  sortDirection->setObjectName("sortDirection");
  sortDirection->setProperty("ascending", true);
  sortDirection->setIcon(style()->standardIcon(QStyle::SP_ArrowUp));
  sortDirection->setToolTip("Ascending");
  sortLayout->addWidget(sortDirection);
  auto *sort = new QComboBox;
  sort->setObjectName("sortSelect");
  sort->addItem("Name", "name");
  sort->addItem("Platinum", "platinum");
  sort->addItem("Ducats", "ducats");
  sort->addItem("Amount", "amount");
  sort->addItem("Ducats / Platinum", "ducanator");
  sort->addItem("Set completion", "complete");
  sortLayout->addWidget(sort);
  toolbar->addWidget(sortControl);

  connect(sort, &QComboBox::currentIndexChanged, this, [this, sort](int index) {
    sortMode_ = sort->itemData(index).toString();
    const bool loadingPrices = priceSort();
    priceLoading_ = loadingPrices;
    items_->setPricesLoading(loadingPrices);
    items_->setSortMode(sortMode_);
    if (loadingPrices) {
      beginPriceLoad();
    } else {
      updateContent();
    }
  });
  connect(sortDirection, &QToolButton::clicked, this, [this, sortDirection] {
    const bool ascending = !sortDirection->property("ascending").toBool();
    sortDirection->setProperty("ascending", ascending);
    sortDirection->setIcon(style()->standardIcon(
        ascending ? QStyle::SP_ArrowUp : QStyle::SP_ArrowDown));
    sortDirection->setToolTip(ascending ? "Ascending" : "Descending");
    items_->setSortAscending(ascending);
  });

  refresh_->setObjectName("compactTool");
  refresh_->setIcon(QIcon(":/resources/ui/refresh.png"));
  refresh_->setIconSize({20, 20});
  refresh_->setToolTip("Refresh inventory data");
  toolbar->addWidget(refresh_);
  layout->addLayout(toolbar);
  updateFilterButtons(groupButtons);

  progress_->setObjectName("priceProgress");
  progress_->setTextVisible(false);
  progress_->setFixedHeight(2);

  emptyState_->setObjectName("emptyState");
  emptyState_->setAlignment(Qt::AlignCenter);
  emptyState_->setWordWrap(true);

  auto *loading = new QWidget;
  auto *loadingLayout = new QVBoxLayout(loading);
  loadingLayout->addStretch();
  auto *loadingProgress = new AnimatedProgressBar;
  loadingProgress->setRange(0, 0);
  loadingProgress->setMaximumWidth(240);
  loadingLayout->addWidget(loadingProgress, 0, Qt::AlignHCenter);
  loadingLayout->addStretch();

  auto *host = new QWidget;
  host->setObjectName("contentHost");
  host->setLayout(content_);
  content_->setContentsMargins(0, 0, 0, 0);
  content_->addWidget(grid_);
  content_->addWidget(loading);
  content_->addWidget(emptyState_);
  auto *frame = new QWidget;
  frame->setObjectName("contentFrame");
  auto *frameLayout = new QVBoxLayout(frame);
  frameLayout->setContentsMargins(0, 0, 0, 0);
  frameLayout->setSpacing(0);
  frameLayout->addWidget(progress_);
  frameLayout->addWidget(host, 1);
  layout->addWidget(frame, 1);

  connect(search, &QLineEdit::textChanged, this, [this](const QString &text) {
    items_->setText(text);
    if (priceSort()) {
      beginPriceLoad();
    } else {
      updateContent();
    }
  });
  connect(groupButtons, &QButtonGroup::idClicked, this,
          [this, groupButtons](int buttonId) {
            items_->setGroup(
                groupButtons->button(buttonId)->property("group").toString());
            updateFilterButtons(groupButtons);
            if (priceSort()) {
              beginPriceLoad();
            } else {
              updateContent();
            }
          });
  connect(refresh_, &QPushButton::clicked, this, [this] {
    if (priceSort()) {
      beginPriceLoad(true);
    } else {
      grid_->refreshVisibleQuotes();
    }
    controller_->refreshInventory();
  });
  connect(grid_, &PlayerItemGridWidget::assetsNeeded, controller_,
          &AppController::resolveAssets);
  connect(grid_, &PlayerItemGridWidget::quotesNeeded, controller_,
          &AppController::resolveMarketQuotes);
  connect(grid_, &PlayerItemGridWidget::marketItemRequested, this,
          &InventoryWidget::marketItemRequested);
  connect(grid_, &PlayerItemGridWidget::relicRewardRequested, this,
          &InventoryWidget::relicRewardRequested);
  connect(controller_, &AppController::inventoryStateChanged, this, [this] {
    if (priceSort() && !controller_->inventoryLoading()) {
      beginPriceLoad();
    } else {
      updateContent();
    }
  });
  connect(items_, &QAbstractItemModel::modelReset, this, [this] {
    if (priceSort()) {
      beginPriceLoad();
    } else {
      updateContent();
    }
  });
  connect(items_, &QAbstractItemModel::rowsInserted, this,
          &InventoryWidget::updateContent);
  connect(items_, &QAbstractItemModel::rowsRemoved, this,
          &InventoryWidget::updateContent);
  priceUpdateTimer_->setInterval(33);
  priceUpdateTimer_->setSingleShot(true);
  connect(priceUpdateTimer_, &QTimer::timeout, this,
          &InventoryWidget::updatePriceLoad);
  connect(controller_, &AppController::marketQuotesChanged, this, [this] {
    if (priceLoading_ && !priceUpdateTimer_->isActive()) {
      priceUpdateTimer_->start();
    }
  });
  updateContent();
}

bool InventoryWidget::priceSort() const {
  return sortMode_ == "platinum" || sortMode_ == "ducanator";
}

void InventoryWidget::beginPriceLoad(bool refresh) {
  if (!priceSort()) {
    return;
  }
  if (!priceLoading_) {
    priceLoading_ = true;
    items_->setPricesLoading(true);
  }
  if (!controller_->inventoryLoading()) {
    grid_->requestAllQuotes(refresh);
  }
  updatePriceLoad();
}

void InventoryWidget::updatePriceLoad() {
  priceTotal_ = 0;
  pendingPrices_ = 0;
  if (priceSort() && !controller_->inventoryLoading()) {
    for (int row = 0; row < items_->rowCount(); ++row) {
      const QModelIndex item = items_->index(row, 0);
      if (!item.data(PlayerItemModel::TradableRole).toBool()) {
        continue;
      }
      ++priceTotal_;
      if (item.data(PlayerItemModel::PriceStateRole).toString() == "loading") {
        ++pendingPrices_;
      }
    }
    if (priceLoading_ && pendingPrices_ == 0) {
      priceLoading_ = false;
      items_->setPricesLoading(false);
    }
  }
  updateContent();
}

void InventoryWidget::updateContent() {
  const bool loading = controller_->inventoryLoading();
  const bool pricing = priceLoading_ && pendingPrices_ > 0;
  if (loading) {
    progress_->setRange(0, 0);
  } else if (pricing) {
    progress_->setRange(0, std::max(1, priceTotal_));
    progress_->setValue(priceTotal_ - pendingPrices_);
  } else {
    progress_->setRange(0, 1);
    progress_->setValue(0);
  }
  refresh_->setEnabled(!loading);
  if (items_->rowCount() > 0) {
    content_->setCurrentIndex(0);
  } else if (loading && !controller_->inventoryLoaded()) {
    content_->setCurrentIndex(1);
  } else {
    emptyState_->setText(controller_->inventoryError().isEmpty()
                             ? "No matching inventory items."
                             : controller_->inventoryError());
    content_->setCurrentIndex(2);
  }
}
