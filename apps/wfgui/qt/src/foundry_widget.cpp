#include "foundry_widget.h"

#include <QAbstractItemModel>
#include <QActionGroup>
#include <QButtonGroup>
#include <QHBoxLayout>
#include <QIcon>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QParallelAnimationGroup>
#include <QPropertyAnimation>
#include <QPushButton>
#include <QStackedLayout>
#include <QStyle>
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

constexpr std::array<Filter, 8> Groups{{
    {"All", "all", ":/resources/categories/all.png"},
    {"Warframe", "warframe", ":/resources/categories/warframe.png"},
    {"Primary", "primary", ":/resources/categories/primary.png"},
    {"Secondary", "secondary", ":/resources/categories/secondary.png"},
    {"Melee", "melee", ":/resources/categories/melee.png"},
    {"Modular", "modular", ":/resources/categories/modular.png"},
    {"Arch", "arch", ":/resources/categories/arch.png"},
    {"Companion", "companion", ":/resources/categories/companion.png"},
}};

constexpr int FilterChipCollapsedWidth = 40;
constexpr int FilterChipAnimationDuration = 250;

void updateFilterButtons(QButtonGroup *group, bool animated = true) {
  if (auto *running = group->findChild<QParallelAnimationGroup *>(
          "filterChipAnimation", Qt::FindDirectChildrenOnly)) {
    running->stop();
    delete running;
  }

  auto *animations = new QParallelAnimationGroup(group);
  animations->setObjectName("filterChipAnimation");
  for (auto *button : group->buttons()) {
    if (button->isChecked()) {
      button->setText(button->property("label").toString());
    }
    const int target = button->isChecked()
                           ? button->property("expandedWidth").toInt()
                           : FilterChipCollapsedWidth;
    if (!animated) {
      button->setMaximumWidth(target);
      if (!button->isChecked()) {
        button->setText({});
      }
    } else if (button->maximumWidth() != target) {
      auto *animation =
          new QPropertyAnimation(button, "maximumWidth", animations);
      animation->setDuration(FilterChipAnimationDuration);
      animation->setStartValue(button->maximumWidth());
      animation->setEndValue(target);
      animation->setEasingCurve(QEasingCurve::InOutCubic);
      animations->addAnimation(animation);
    } else if (!button->isChecked()) {
      button->setText({});
    }
  }
  if (animations->animationCount() == 0) {
    delete animations;
  } else {
    QObject::connect(animations, &QParallelAnimationGroup::finished, group,
                     [group] {
                       for (auto *button : group->buttons()) {
                         if (!button->isChecked()) {
                           button->setText({});
                         }
                       }
                     });
    animations->start(QAbstractAnimation::DeleteWhenStopped);
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

FoundryWidget::FoundryWidget(AppController *controller, QWidget *parent)
    : QWidget(parent), controller_(controller),
      items_(new PlayerItemFilterModel(this)),
      grid_(new PlayerItemGridWidget(PlayerItemGridWidget::Kind::Foundry)),
      groups_(new QButtonGroup(this)),
      search_(new CompactSearch("Search Foundry")), summary_(new QLabel),
      emptyState_(new QLabel), progress_(new AnimatedProgressBar),
      refresh_(new QPushButton), content_(new QStackedLayout) {
  setObjectName("page");
  items_->setSourceModel(controller_->foundryItems());
  items_->setGroup("warframe");
  grid_->setModel(items_);

  auto *layout = new QVBoxLayout(this);
  layout->setContentsMargins(10, 10, 10, 0);
  layout->setSpacing(8);

  auto *toolbar = new QHBoxLayout;
  toolbar->setSpacing(10);
  auto *groups = groups_;
  groups->setExclusive(true);
  int id = 0;
  for (const auto &[label, value, icon] : Groups) {
    auto *button = new QPushButton;
    button->setObjectName("filterChip");
    button->setCheckable(true);
    button->setProperty("group", value);
    button->setProperty("label", label);
    button->setProperty("animated", true);
    button->setText(label);
    button->setIcon(QIcon(icon));
    button->setIconSize({22, 22});
    button->setToolTip(label);
    button->setChecked(id == 1);
    button->ensurePolished();
    button->setProperty("expandedWidth", std::max(FilterChipCollapsedWidth,
                                                  button->sizeHint().width()));
    groups->addButton(button, id++);
    toolbar->addWidget(button);
  }
  toolbar->addStretch();
  auto *expandingSearch = search_;
  auto *search = expandingSearch->editor();
  toolbar->addWidget(expandingSearch);

  auto *filter = new QToolButton;
  filter->setObjectName("compactTool");
  filter->setIcon(QIcon(":/resources/ui/filter.png"));
  filter->setIconSize({17, 17});
  filter->setToolTip("Filter Foundry");
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
      {"Owned", "owned", "Owned", "Not owned"},
      {"Ready to build", "ready", "Ready", "Not ready"},
      {"Favorite", "favorite", "Favorite", "Not favorite"},
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
              updateContent();
            });
  }
  toolbar->addWidget(filter);

  refresh_->setObjectName("compactTool");
  refresh_->setIcon(QIcon(":/resources/ui/refresh.png"));
  refresh_->setIconSize({20, 20});
  refresh_->setToolTip("Refresh Foundry data");
  toolbar->addWidget(refresh_);
  layout->addLayout(toolbar);
  updateFilterButtons(groups, false);

  summary_->setObjectName("secondaryText");

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

  connect(search, &QLineEdit::textChanged, items_,
          &PlayerItemFilterModel::setText);
  connect(search, &QLineEdit::textChanged, this, &FoundryWidget::updateContent);
  connect(groups, &QButtonGroup::idClicked, this, [this, groups](int buttonId) {
    items_->setGroup(groups->button(buttonId)->property("group").toString());
    updateFilterButtons(groups);
    updateContent();
  });
  connect(refresh_, &QPushButton::clicked, controller_,
          &AppController::refreshFoundry);
  connect(grid_, &PlayerItemGridWidget::assetsNeeded, controller_,
          &AppController::resolveAssets);
  connect(grid_, &PlayerItemGridWidget::marketItemRequested, this,
          &FoundryWidget::marketItemRequested);
  connect(grid_, &PlayerItemGridWidget::relicRewardRequested, this,
          &FoundryWidget::relicRewardRequested);
  connect(controller_, &AppController::foundryStateChanged, this,
          &FoundryWidget::updateContent);
  connect(items_, &QAbstractItemModel::modelReset, this,
          &FoundryWidget::updateContent);
  connect(items_, &QAbstractItemModel::rowsInserted, this,
          &FoundryWidget::updateContent);
  connect(items_, &QAbstractItemModel::rowsRemoved, this,
          &FoundryWidget::updateContent);
  updateContent();
}

void FoundryWidget::showItem(const QString &item) {
  items_->setGroup("all");
  items_->setText(item);
  search_->setText(item);
  for (auto *button : groups_->buttons()) {
    button->setChecked(button->property("group").toString() == "all");
  }
  updateFilterButtons(groups_, false);
  updateContent();
}

void FoundryWidget::updateContent() {
  const QJsonObject totals = controller_->foundrySummary();
  summary_->setText(QString("%1 items  ·  %2 ready  ·  %3 owned")
                        .arg(totals.value("total").toInt())
                        .arg(totals.value("ready").toInt())
                        .arg(totals.value("owned").toInt()));
  const bool loading = controller_->foundryLoading();
  progress_->setRange(0, loading ? 0 : 1);
  if (!loading) {
    progress_->setValue(0);
  }
  refresh_->setEnabled(!loading);
  if (items_->rowCount() > 0) {
    content_->setCurrentIndex(0);
  } else if (loading && !controller_->foundryLoaded()) {
    content_->setCurrentIndex(1);
  } else {
    emptyState_->setText(controller_->foundryError().isEmpty()
                             ? "No matching Foundry items."
                             : controller_->foundryError());
    content_->setCurrentIndex(2);
  }
}
