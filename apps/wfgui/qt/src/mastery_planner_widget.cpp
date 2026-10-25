#include "mastery_planner_widget.h"
#include "thumbnail_widget.h"

#include <QAbstractItemModel>
#include <QButtonGroup>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QIcon>
#include <QJsonObject>
#include <QLabel>
#include <QProgressBar>
#include <QPushButton>
#include <QSizePolicy>
#include <QStackedLayout>
#include <QTimer>
#include <QVBoxLayout>

#include <array>
#include <utility>

#include "animated_progress_bar.h"
#include "app_controller.h"
#include "player_item_grid_widget.h"
#include "player_item_model.h"

namespace {
struct SummaryRow {
  const char *label;
  const char *key;
};

struct Mode {
  const char *label;
  const char *value;
  const char *icon;
};

constexpr std::array<Mode, 3> Modes{{
    {"Easy", "easy", ":/resources/ui/mastery_easy.png"},
    {"From relics", "relics", ":/resources/ui/mastery_relics.png"},
    {"With platinum", "platinum", ":/assets/platinum.png"},
}};

int currentValue(const QJsonObject &value) {
  return value.contains("mastered") ? value.value("mastered").toInt()
                                    : value.value("current").toInt();
}

QString percentText(const QJsonObject &value) {
  if (value.value("percent").isDouble()) {
    return QString("%1%").arg(value.value("percent").toInt());
  }
  if (!value.value("total").isDouble()) {
    return "--";
  }
  const int total = value.value("total").toInt();
  return QString("%1%").arg(total > 0 ? currentValue(value) * 100 / total : 0);
}

QString progressText(const QJsonObject &value) {
  const QString total = value.value("total").isDouble()
                            ? QString::number(value.value("total").toInt())
                            : QString("--");
  return QString("%1 / %2").arg(currentValue(value)).arg(total);
}
} // namespace

class MasterySummaryPanel final : public QWidget {
public:
  MasterySummaryPanel(const QString &title, const QString &icon,
                      std::initializer_list<SummaryRow> rows, int minimumWidth,
                      QWidget *parent = nullptr)
      : QWidget(parent), percent_(new QLabel) {
    setObjectName("masterySummaryPanel");
    setMinimumWidth(minimumWidth);
    setFixedHeight(151);
    setSizePolicy(QSizePolicy::Preferred, QSizePolicy::Fixed);

    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 8);
    layout->setSpacing(4);
    auto *header = new QWidget;
    header->setObjectName("masterySummaryHeader");
    auto *headerLayout = new QHBoxLayout(header);
    headerLayout->setContentsMargins(10, 5, 10, 5);
    headerLayout->setSpacing(7);
    headerLayout->addStretch();
    auto *iconLabel = new ThumbnailWidget;
    iconLabel->setFixedSize(30, 30);
    iconLabel->setImageBounds(QSize(28, 28));
    iconLabel->setAsset(wfgui::AssetRef::embedded(icon, icon));
    headerLayout->addWidget(iconLabel);
    auto *heading = new QLabel(title);
    heading->setObjectName("masterySummaryHeading");
    headerLayout->addWidget(heading);
    percent_->setObjectName("masterySummaryPercent");
    headerLayout->addWidget(percent_);
    headerLayout->addStretch();
    layout->addWidget(header);

    auto *details = new QGridLayout;
    details->setContentsMargins(10, 5, 10, 2);
    details->setHorizontalSpacing(7);
    details->setVerticalSpacing(4);
    int row = 0;
    for (const SummaryRow &definition : rows) {
      keys_.append(QString::fromLatin1(definition.key));
      auto *label = new QLabel(QString::fromLatin1(definition.label));
      label->setObjectName("masterySummaryRowLabel");
      label->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
      auto *rowPercent = new QLabel;
      rowPercent->setObjectName("masterySummaryRowValue");
      rowPercent->setAlignment(Qt::AlignCenter);
      auto *progress = new QLabel;
      progress->setObjectName("masterySummaryRowValue");
      progress->setAlignment(Qt::AlignCenter);
      rowPercents_.append(rowPercent);
      progresses_.append(progress);
      details->addWidget(label, row, 0);
      details->addWidget(rowPercent, row, 1);
      details->addWidget(progress, row, 2);
      details->setRowStretch(row, 1);
      ++row;
    }
    details->setColumnStretch(0, 1);
    layout->addLayout(details, 1);
  }

  void setData(const QJsonObject &data) {
    percent_->setText(data.value("percent").isDouble()
                          ? QString("%1%").arg(data.value("percent").toInt())
                          : QString("--"));
    for (int row = 0; row < keys_.size(); ++row) {
      const QJsonObject value = data.value(keys_.at(row)).toObject();
      rowPercents_.at(row)->setText(percentText(value));
      progresses_.at(row)->setText(progressText(value));
    }
  }

private:
  QLabel *percent_;
  QStringList keys_;
  QList<QLabel *> rowPercents_;
  QList<QLabel *> progresses_;
};

MasteryPlannerWidget::MasteryPlannerWidget(AppController *controller,
                                           QWidget *parent)
    : QWidget(parent), controller_(controller),
      items_(new PlayerItemFilterModel(this)),
      grid_(new PlayerItemGridWidget(PlayerItemGridWidget::Kind::Mastery)),
      rank_(new QLabel), rankIcon_(new ThumbnailWidget),
      completionPercent_(new QLabel), completionText_(new QLabel),
      gameContent_(new MasterySummaryPanel(
          "Game Content", ":/resources/ui/summary_game.png",
          {{"Warframes / Archwings:", "warframes"},
           {"Weapons:", "weapons"},
           {"Companions:", "companions"}},
          290)),
      starChart_(new MasterySummaryPanel(
          "Star Chart", ":/resources/ui/summary_star.png",
          {{"Normal:", "normal"},
           {"Junctions:", "junctions"},
           {"Steel Path:", "steel"},
           {"Steel P. junctions:", "steel_junctions"}},
          252)),
      intrinsics_(new MasterySummaryPanel(
          "Intrinsics", ":/resources/ui/summary_intrinsics.png",
          {{"Railjack:", "railjack"}, {"Duviri:", "duviri"}}, 180)),
      emptyState_(new QLabel), completionBar_(new QProgressBar),
      loadingBar_(new AnimatedProgressBar), refresh_(new QPushButton),
      content_(new QStackedLayout), priceUpdateTimer_(new QTimer(this)) {
  setObjectName("page");
  items_->setSourceModel(controller_->masteryItems());
  items_->setMode("easy");
  grid_->setModel(items_);

  auto *layout = new QVBoxLayout(this);
  layout->setContentsMargins(9, 9, 9, 0);
  layout->setSpacing(7);

  auto *top = new QWidget;
  top->setObjectName("masteryTop");
  auto *topLayout = new QHBoxLayout(top);
  topLayout->setContentsMargins(0, 0, 0, 0);
  topLayout->setSpacing(9);
  rank_->setObjectName("masteryRank");
  rank_->setAlignment(Qt::AlignCenter);
  topLayout->addWidget(rank_);
  rankIcon_->setObjectName("masteryRankIcon");
  rankIcon_->setFixedSize(60, 60);
  topLayout->addWidget(rankIcon_);
  topLayout->addSpacing(10);

  auto *rankProgress = new QVBoxLayout;
  rankProgress->setSpacing(5);
  auto *progressLabels = new QHBoxLayout;
  completionPercent_->setObjectName("masteryProgressLabel");
  completionText_->setObjectName("masteryProgressLabel");
  progressLabels->addWidget(completionPercent_);
  progressLabels->addStretch();
  progressLabels->addWidget(completionText_);
  refresh_->setObjectName("compactTool");
  refresh_->setIcon(QIcon(":/resources/ui/refresh.png"));
  refresh_->setIconSize({18, 18});
  refresh_->setToolTip("Refresh mastery data");
  progressLabels->addWidget(refresh_);
  rankProgress->addLayout(progressLabels);
  completionBar_->setObjectName("masteryProgress");
  completionBar_->setTextVisible(false);
  completionBar_->setRange(0, 100);
  rankProgress->addWidget(completionBar_);
  topLayout->addLayout(rankProgress, 1);
  layout->addWidget(top);

  auto *summary = new QHBoxLayout;
  summary->setContentsMargins(10, 0, 10, 0);
  summary->setSpacing(0);
  summary->addStretch(1);
  summary->addWidget(gameContent_);
  summary->addStretch(2);
  summary->addWidget(starChart_);
  summary->addStretch(2);
  summary->addWidget(intrinsics_);
  summary->addStretch(1);
  layout->addLayout(summary);

  auto *bottom = new QWidget;
  bottom->setObjectName("masteryBottom");
  auto *bottomLayout = new QVBoxLayout(bottom);
  bottomLayout->setContentsMargins(7, 7, 7, 0);
  bottomLayout->setSpacing(8);
  auto *bottomHeader = new QHBoxLayout;
  bottomHeader->setContentsMargins(4, 0, 0, 0);
  bottomHeader->setAlignment(Qt::AlignVCenter);
  auto *sectionTitle = new QLabel("Best ways to level up mastery:");
  sectionTitle->setObjectName("masterySectionTitle");
  bottomHeader->addWidget(sectionTitle);
  bottomHeader->addStretch();
  auto *modeButtons = new QButtonGroup(this);
  modeButtons->setExclusive(true);
  auto *modeGroup = new QWidget;
  modeGroup->setObjectName("masteryModeGroup");
  auto *modeLayout = new QHBoxLayout(modeGroup);
  modeLayout->setContentsMargins(0, 0, 0, 0);
  modeLayout->setSpacing(2);
  int id = 0;
  for (const Mode &mode : Modes) {
    auto *button = new QPushButton(mode.label);
    button->setObjectName("masteryMode");
    button->setIcon(QIcon(mode.icon));
    button->setIconSize({20, 20});
    button->setCheckable(true);
    button->setProperty("mode", mode.value);
    button->setChecked(id == 0);
    modeButtons->addButton(button, id++);
    modeLayout->addWidget(button);
  }
  bottomHeader->addWidget(modeGroup);

  loadingBar_->setObjectName("priceProgress");
  loadingBar_->setRange(0, 1);
  loadingBar_->setValue(0);
  loadingBar_->setTextVisible(false);
  loadingBar_->setFixedHeight(2);
  QSizePolicy loadingPolicy = loadingBar_->sizePolicy();
  loadingPolicy.setRetainSizeWhenHidden(true);
  loadingBar_->setSizePolicy(loadingPolicy);
  loadingBar_->hide();
  auto *loadingRow = new QHBoxLayout;
  loadingRow->setContentsMargins(4, 0, 4, 0);
  loadingRow->addWidget(loadingBar_);
  bottomLayout->addLayout(loadingRow);
  bottomLayout->addLayout(bottomHeader);
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
  bottomLayout->addWidget(host, 1);
  layout->addWidget(bottom, 1);

  connect(modeButtons, &QButtonGroup::idClicked, this,
          [this, modeButtons](int buttonId) {
            setMode(modeButtons->button(buttonId)->property("mode").toString());
          });
  connect(refresh_, &QPushButton::clicked, controller_,
          &AppController::refreshMastery);
  connect(grid_, &PlayerItemGridWidget::assetsNeeded, controller_,
          &AppController::resolveAssets);
  connect(grid_, &PlayerItemGridWidget::quotesNeeded, controller_,
          &AppController::resolveMarketQuotes);
  connect(grid_, &PlayerItemGridWidget::marketItemRequested, this,
          &MasteryPlannerWidget::marketItemRequested);
  connect(grid_, &PlayerItemGridWidget::relicRewardRequested, this,
          &MasteryPlannerWidget::relicRewardRequested);
  connect(grid_, &PlayerItemGridWidget::foundryItemRequested, this,
          &MasteryPlannerWidget::foundryItemRequested);
  connect(controller_, &AppController::masteryStateChanged, this, [this] {
    if (mode_ == "platinum" && priceLoading_) {
      updatePriceLoad();
    } else {
      updateContent();
    }
  });
  connect(controller_, &AppController::playerProfileChanged, this,
          &MasteryPlannerWidget::updateContent);
  connect(controller_, &AppController::assetsChanged, this,
          [this](const QStringList &ids) {
            const QString rankAssetId = controller_->playerProfile()
                                            .value("rank_asset")
                                            .toObject()
                                            .value("id")
                                            .toString();
            if (ids.contains(rankAssetId)) {
              updateContent();
            }
          });
  priceUpdateTimer_->setInterval(33);
  priceUpdateTimer_->setSingleShot(true);
  connect(priceUpdateTimer_, &QTimer::timeout, this,
          &MasteryPlannerWidget::updatePriceLoad);
  connect(controller_, &AppController::marketQuotesChanged, this, [this] {
    if (mode_ == "platinum" && priceLoading_ &&
        !priceUpdateTimer_->isActive()) {
      priceUpdateTimer_->start();
    }
  });
  connect(controller_, &AppController::marketQuoteCacheSettled, this, [this] {
    if (mode_ == "platinum" && priceLoading_) {
      items_->sort(0);
      updateContent();
    }
  });
  connect(controller_, &AppController::marketQuoteFetchSettled, this, [this] {
    if (mode_ == "platinum" && priceLoading_) {
      updatePriceLoad();
    }
  });
  connect(items_, &QAbstractItemModel::modelReset, this, [this] {
    if (mode_ == "platinum") {
      beginPriceLoad();
    }
    updateContent();
  });
  connect(items_, &QAbstractItemModel::rowsInserted, this,
          &MasteryPlannerWidget::updateContent);
  connect(items_, &QAbstractItemModel::rowsRemoved, this,
          &MasteryPlannerWidget::updateContent);
  updateContent();
}

void MasteryPlannerWidget::setMode(const QString &mode) {
  if (mode_ == mode) {
    return;
  }
  mode_ = mode;
  priceLoading_ = mode == "platinum";
  items_->setPricesLoading(priceLoading_);
  items_->setMode(mode_);
  if (priceLoading_) {
    beginPriceLoad();
  }
  updateContent();
}

void MasteryPlannerWidget::beginPriceLoad() {
  if (mode_ != "platinum") {
    return;
  }
  if (!priceLoading_) {
    priceLoading_ = true;
    items_->setPricesLoading(true);
  }
  grid_->requestAllQuotes();
  updatePriceLoad();
}

void MasteryPlannerWidget::updatePriceLoad() {
  if (mode_ != "platinum" || controller_->masteryLoading() ||
      controller_->marketQuoteFetchBusy()) {
    updateContent();
    return;
  }

  int pending = 0;
  const int total = items_->rowCount();
  for (int row = 0; row < total; ++row) {
    if (items_->index(row, 0)
            .data(PlayerItemModel::AcquisitionPriceStateRole)
            .toString() == "loading") {
      ++pending;
    }
  }

  if (priceLoading_ && pending == 0) {
    priceLoading_ = false;
    items_->setPricesLoading(false);
  }
  updateContent();
}

void MasteryPlannerWidget::updateContent() {
  const QJsonObject summary = controller_->masterySummary();
  const QJsonValue level = summary.value("player_level");
  rank_->setText(level.isDouble() ? QString::number(level.toInt())
                                  : QString("--"));
  const QJsonObject profile = controller_->playerProfile();
  const QString rankAssetId =
      profile.value("rank_asset").toObject().value("id").toString();
  const auto asset = controller_->assetRef(rankAssetId);
  rankIcon_->setAsset(
      asset.isValid() ? asset
                      : wfgui::AssetRef::embedded(
                            "mastery_rank", ":/resources/ui/mastery_rank.png"));
  const QJsonObject rankProgress = summary.value("rank_progress").toObject();
  const bool hasRankProgress = rankProgress.value("available").toBool() &&
                               rankProgress.value("current").isDouble() &&
                               rankProgress.value("total").isDouble();
  const int rankPercent =
      hasRankProgress ? rankProgress.value("percent").toInt() : 0;
  completionPercent_->setText(hasRankProgress ? QString("%1%").arg(rankPercent)
                                              : QString("--"));
  completionText_->setText(hasRankProgress
                               ? QString("%1 / %2 XP")
                                     .arg(rankProgress.value("current").toInt())
                                     .arg(rankProgress.value("total").toInt())
                               : QString("Mastery XP unavailable"));
  completionBar_->setValue(rankPercent);

  QJsonObject game;
  game.insert("percent", summary.value("content_percent"));
  game.insert("warframes", summary.value("warframes"));
  game.insert("weapons", summary.value("weapons"));
  game.insert("companions", summary.value("companions"));
  gameContent_->setData(game);
  starChart_->setData(summary.value("star_chart").toObject());
  intrinsics_->setData(summary.value("intrinsics").toObject());

  const bool loading = controller_->masteryLoading();
  const bool pricing = priceLoading_ && !loading;
  loadingBar_->setVisible(loading || pricing);
  if (loading) {
    loadingBar_->setRange(0, 0);
  } else if (pricing) {
    loadingBar_->setRange(0, 0);
  } else {
    loadingBar_->setRange(0, 1);
    loadingBar_->setValue(0);
  }
  refresh_->setEnabled(!loading);
  if (items_->rowCount() > 0) {
    content_->setCurrentIndex(0);
  } else if (loading && !controller_->masteryLoaded()) {
    content_->setCurrentIndex(1);
  } else {
    emptyState_->setText(controller_->masteryError().isEmpty()
                             ? "No matching mastery items."
                             : controller_->masteryError());
    content_->setCurrentIndex(2);
  }
}
