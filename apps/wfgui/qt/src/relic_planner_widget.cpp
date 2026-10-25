#include "relic_planner_widget.h"

#include <QAbstractItemModel>
#include <QButtonGroup>
#include <QCheckBox>
#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QStackedLayout>
#include <QVBoxLayout>

#include <array>
#include <utility>

#include "animated_progress_bar.h"
#include "app_controller.h"
#include "compact_search.h"
#include "relic_grid_widget.h"
#include "relic_model.h"

namespace {
constexpr std::array<std::pair<const char *, const char *>, 6> Eras{{
    {"All", "all"},
    {"Lith", "lith"},
    {"Meso", "meso"},
    {"Neo", "neo"},
    {"Axi", "axi"},
    {"Requiem", "requiem"},
}};
} // namespace

RelicPlannerWidget::RelicPlannerWidget(AppController *controller,
                                       QWidget *parent)
    : QWidget(parent), controller_(controller), traceCount_(new QLabel),
      priceProgress_(new AnimatedProgressBar), emptyState_(new QLabel),
      relics_(new RelicGridWidget),
      search_(new CompactSearch("Search relics or rewards")),
      refresh_(new QPushButton), eraGroup_(new QButtonGroup(this)),
      content_(new QStackedLayout) {
  setObjectName("page");
  auto *layout = new QVBoxLayout(this);
  layout->setContentsMargins(10, 10, 10, 0);
  layout->setSpacing(8);

  auto *toolbar = new QHBoxLayout;
  toolbar->setSpacing(6);
  traceCount_->setObjectName("secondaryText");
  eraGroup_->setExclusive(true);
  int eraId = 0;
  for (const auto &[label, value] : Eras) {
    auto *button = new QPushButton;
    button->setObjectName("filterChip");
    button->setCheckable(true);
    button->setProperty("era", value);
    button->setProperty("label", label);
    button->setToolTip(label);
    if (QString::fromLatin1(value) == "all") {
      button->setIcon(QIcon(":/resources/categories/all.png"));
      button->setIconSize({22, 22});
    } else if (QString::fromLatin1(value) == "requiem") {
      button->setIcon(QIcon(":/resources/activity/requiem.png"));
      button->setIconSize({22, 22});
    }
    eraGroup_->addButton(button, eraId++);
    toolbar->addWidget(button);
  }
  toolbar->addWidget(traceCount_);
  toolbar->addStretch();
  auto *filter = search_->editor();
  toolbar->addWidget(search_);
  auto *onlyOwned = new QCheckBox("Only owned");
  onlyOwned->setObjectName("ownedToggle");
  onlyOwned->setChecked(controller_->onlyOwned());
  toolbar->addWidget(onlyOwned);
  refresh_->setObjectName("compactTool");
  refresh_->setIcon(QIcon(":/resources/ui/refresh.png"));
  refresh_->setIconSize({20, 20});
  refresh_->setToolTip("Refresh relic data");
  toolbar->addWidget(refresh_);
  layout->addLayout(toolbar);

  priceProgress_->setObjectName("priceProgress");
  priceProgress_->setRange(0, 1);
  priceProgress_->setValue(0);
  priceProgress_->setTextVisible(false);
  priceProgress_->setFixedHeight(2);

  relics_->setModel(controller_->relics());

  auto *loading = new QWidget;
  auto *loadingLayout = new QVBoxLayout(loading);
  loadingLayout->addStretch();
  auto *progress = new AnimatedProgressBar;
  progress->setRange(0, 0);
  progress->setMaximumWidth(240);
  loadingLayout->addWidget(progress, 0, Qt::AlignHCenter);
  loadingLayout->addStretch();

  emptyState_->setObjectName("emptyState");
  emptyState_->setAlignment(Qt::AlignCenter);
  emptyState_->setWordWrap(true);

  auto *contentFrame = new QWidget;
  contentFrame->setObjectName("contentFrame");
  auto *contentHost = new QWidget;
  contentHost->setObjectName("contentHost");
  contentHost->setLayout(content_);
  content_->setContentsMargins(0, 0, 0, 0);
  content_->addWidget(relics_);
  content_->addWidget(loading);
  content_->addWidget(emptyState_);
  auto *contentLayout = new QVBoxLayout(contentFrame);
  contentLayout->setContentsMargins(0, 0, 0, 0);
  contentLayout->setSpacing(0);
  contentLayout->addWidget(priceProgress_);
  contentLayout->addWidget(contentHost, 1);
  layout->addWidget(contentFrame, 1);

  connect(filter, &QLineEdit::textChanged, controller_,
          &AppController::setFilterText);
  connect(refresh_, &QPushButton::clicked, controller_,
          &AppController::refresh);
  connect(onlyOwned, &QCheckBox::toggled, controller_,
          &AppController::setOnlyOwned);
  connect(eraGroup_, &QButtonGroup::idClicked, this,
          [this, controller](int id) {
            controller->selectEra(
                eraGroup_->button(id)->property("era").toString());
          });
  connect(controller_, &AppController::selectedEraChanged, this,
          &RelicPlannerWidget::updateEra);
  connect(controller_, &AppController::onlyOwnedChanged, this,
          &RelicPlannerWidget::updateContent);
  connect(relics_, &RelicGridWidget::marketItemRequested, this,
          &RelicPlannerWidget::marketItemRequested);
  connect(relics_, &RelicGridWidget::rewardFilterRequested, this,
          &RelicPlannerWidget::showReward);
  connect(relics_, &RelicGridWidget::foundryItemRequested, this,
          &RelicPlannerWidget::foundryItemRequested);
  connect(controller_, &AppController::loadingChanged, this,
          &RelicPlannerWidget::updateContent);
  connect(controller_, &AppController::pricingChanged, this,
          &RelicPlannerWidget::updateContent);
  connect(controller_, &AppController::errorChanged, this,
          &RelicPlannerWidget::updateContent);
  connect(controller_, &AppController::traceCountChanged, this,
          &RelicPlannerWidget::updateContent);
  connect(controller_->relics(), &QAbstractItemModel::modelReset, this, [this] {
    updateEraIcons();
    updateContent();
  });
  connect(controller_->relics(), &QAbstractItemModel::dataChanged, this,
          [this] { updateEraIcons(); });
  connect(controller_->relics(), &QAbstractItemModel::rowsInserted, this,
          &RelicPlannerWidget::updateContent);
  connect(controller_->relics(), &QAbstractItemModel::rowsRemoved, this,
          &RelicPlannerWidget::updateContent);

  updateEra();
  updateContent();
}

void RelicPlannerWidget::showEra(const QString &era) {
  search_->setText({});
  controller_->selectEra(era);
}

void RelicPlannerWidget::showReward(const QString &reward) {
  controller_->selectEra("all");
  search_->setText(reward);
}

void RelicPlannerWidget::updateContent() {
  traceCount_->setText(
      QString("%1 Void Traces").arg(controller_->traceCount()));
  const bool pricing =
      controller_->pricing() && controller_->relics()->rowCount() > 0;
  priceProgress_->setRange(0, pricing ? 0 : 1);
  if (!pricing) {
    priceProgress_->setValue(0);
  }
  refresh_->setEnabled(!controller_->loading());

  const int count = controller_->relics()->rowCount();
  if (count > 0) {
    content_->setCurrentIndex(0);
  } else if (controller_->loading()) {
    content_->setCurrentIndex(1);
  } else {
    emptyState_->setText(
        controller_->error().isEmpty()
            ? (controller_->onlyOwned()
                   ? QString("No owned relics found for this era.")
                   : QString("No relics found for this era."))
            : controller_->error());
    content_->setCurrentIndex(2);
  }
}

void RelicPlannerWidget::updateEra() {
  for (auto *button : eraGroup_->buttons()) {
    const bool selected =
        button->property("era").toString() == controller_->selectedEra();
    button->setChecked(selected);
    const QString label = button->property("label").toString();
    button->setText(selected || button->icon().isNull() ? label : QString());
  }
}

void RelicPlannerWidget::updateEraIcons() {
  for (int row = 0; row < controller_->relics()->rowCount(); ++row) {
    const QModelIndex index = controller_->relics()->index(row, 0);
    const QString era = index.data(RelicModel::EraRole).toString();
    const QString path = index.data(RelicModel::RelicImageRole).toString();
    if (path.isEmpty()) {
      continue;
    }
    for (auto *button : eraGroup_->buttons()) {
      if (button->property("era").toString() == era &&
          button->icon().isNull()) {
        button->setIcon(QIcon(path));
        button->setIconSize({22, 22});
      }
    }
  }
  updateEra();
}
