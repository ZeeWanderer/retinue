#include "main_window.h"

#include <QAbstractAnimation>
#include <QApplication>
#include <QButtonGroup>
#include <QCoreApplication>
#include <QDir>
#include <QEasingCurve>
#include <QEvent>
#include <QHBoxLayout>
#include <QIcon>
#include <QMouseEvent>
#include <QPainter>
#include <QParallelAnimationGroup>
#include <QPixmap>
#include <QProcess>
#include <QPropertyAnimation>
#include <QPushButton>
#include <QResizeEvent>
#include <QSizePolicy>
#include <QStackedWidget>
#include <QStyle>
#include <QVBoxLayout>
#include <QWidget>
#include <QWindow>

#include <array>

#include "activity_rail_widget.h"
#include "build_planner_widget.h"
#include "display_scale.h"
#include "foundry_widget.h"
#include "inventory_widget.h"
#include "market_item_dialog.h"
#include "market_widget.h"
#include "mastery_planner_widget.h"
#include "player_identity_widget.h"
#include "relic_planner_widget.h"
#include "settings_widget.h"
#include "title_bar_widget.h"
#include "widget_capture.h"

namespace {
constexpr int MinimumWindowWidth = 1024;
constexpr int ActivityRailThreshold = 1380;

enum PageIndex {
  FoundryPage,
  MasteryPage,
  InventoryPage,
  RelicPage,
  BuildPlannerPage,
  MarketPage,
  SettingsPage,
  PageCount
};

const QStringList &pageNames() {
  static const QStringList Names = {"foundry", "mastery",       "inventory",
                                    "relic",   "build-planner", "market",
                                    "settings"};
  return Names;
}

QPixmap tintedPixmap(const QString &path, const QColor &color) {
  const QPixmap source(path);
  QPixmap result(source.size());
  result.fill(Qt::transparent);
  QPainter painter(&result);
  painter.drawPixmap(0, 0, source);
  painter.setCompositionMode(QPainter::CompositionMode_SourceIn);
  painter.fillRect(result.rect(), color);
  return result;
}

QIcon navigationIcon(const QString &path) {
  QIcon icon;
  icon.addPixmap(tintedPixmap(path, QColor("#8a8c95")), QIcon::Normal,
                 QIcon::Off);
  icon.addPixmap(tintedPixmap(path, QColor("#d8dbea")), QIcon::Active,
                 QIcon::Off);
  icon.addPixmap(tintedPixmap(path, Qt::white), QIcon::Normal, QIcon::On);
  return icon;
}

class WindowResizeHandle final : public QWidget {
public:
  WindowResizeHandle(Qt::Edges edges, QWidget *parent)
      : QWidget(parent), edges_(edges) {
    if (edges == Qt::LeftEdge || edges == Qt::RightEdge) {
      setCursor(Qt::SizeHorCursor);
    } else if (edges == Qt::TopEdge || edges == Qt::BottomEdge) {
      setCursor(Qt::SizeVerCursor);
    } else if (edges == (Qt::TopEdge | Qt::LeftEdge) ||
               edges == (Qt::BottomEdge | Qt::RightEdge)) {
      setCursor(Qt::SizeFDiagCursor);
    } else {
      setCursor(Qt::SizeBDiagCursor);
    }
  }

protected:
  void mousePressEvent(QMouseEvent *event) override {
    if (event->button() == Qt::LeftButton && window()->windowHandle()) {
      window()->windowHandle()->startSystemResize(edges_);
      event->accept();
      return;
    }
    QWidget::mousePressEvent(event);
  }

private:
  Qt::Edges edges_;
};
} // namespace

MainWindow::MainWindow(QWidget *parent)
    : QMainWindow(parent), controller_(this), sidebar_(new QWidget),
      playerIdentity_(new PlayerIdentityWidget(&controller_)),
      navigation_(new QButtonGroup(this)), pages_(new QStackedWidget),
      buildPlanner_(new BuildPlannerWidget(&controller_)),
      activityRail_(new ActivityRailWidget(&controller_)),
      marketDialog_(new MarketItemDialog(&controller_, this)),
      titleBar_(new TitleBarWidget) {
  setWindowFlag(Qt::FramelessWindowHint);
  setWindowTitle("wfcli");
  resize(1280, 800);
  setMinimumSize(MinimumWindowWidth, 600);
  wfgui::setCaptureTarget(this, "window");
  wfgui::setCaptureTarget(titleBar_, "titlebar");

  auto *windowRoot = new QWidget;
  windowRoot->setObjectName("windowRoot");
  auto *windowLayout = new QVBoxLayout(windowRoot);
  windowLayout->setContentsMargins(0, 0, 0, 0);
  windowLayout->setSpacing(0);
  windowLayout->addWidget(titleBar_);
  auto *central = new QWidget;
  central->setObjectName("appRoot");
  auto *layout = new QHBoxLayout(central);
  layout->setContentsMargins(6, 6, 6, 0);
  layout->setSpacing(7);

  sidebar_->setObjectName("sidebar");
  wfgui::setCaptureTarget(sidebar_, "left-rail");
  wfgui::setCaptureTarget(playerIdentity_, "left-rail.player");
  sidebar_->setFixedWidth(142);
  auto *sidebarLayout = new QVBoxLayout(sidebar_);
  sidebarLayout->setContentsMargins(8, 14, 8, 12);
  sidebarLayout->setSpacing(8);

  sidebarLayout->addWidget(playerIdentity_);
  sidebarLayout->addSpacing(7);

  navigation_->setExclusive(true);
  struct NavigationItem {
    const char *label;
    const char *icon;
  };
  constexpr std::array<NavigationItem, PageCount> items{{
      {"Foundry", ":/resources/ui/nav_foundry.png"},
      {"Mastery\nHelper", ":/resources/ui/nav_mastery.png"},
      {"Inventory", ":/resources/ui/nav_inventory.png"},
      {"Relic\nPlanner", ":/resources/ui/nav_relic.png"},
      {"Build\nPlanner", ":/resources/ui/nav_builds.png"},
      {"Market", ":/resources/ui/market.png"},
      {"Settings", ":/resources/ui/settings.png"},
  }};
  for (int page = 0; page < static_cast<int>(items.size()); ++page) {
    if (page == SettingsPage) {
      sidebarLayout->addStretch();
    }
    navigationLabels_.append(items.at(page).label);
    auto *button = new QPushButton(items.at(page).label);
    button->setObjectName("navigation");
    button->setIcon(navigationIcon(items.at(page).icon));
    button->setIconSize({24, 24});
    button->setCheckable(true);
    button->setChecked(page == 0);
    button->setProperty("page", page);
    button->setProperty("multiline",
                        QString(items.at(page).label).contains('\n'));
    navigation_->addButton(button, page);
    navigationButtons_.append(button);
    sidebarLayout->addWidget(button);
  }

  layout->addWidget(sidebar_);
  pages_->setObjectName("centerRail");
  auto *foundry = new FoundryWidget(&controller_);
  auto *mastery = new MasteryPlannerWidget(&controller_);
  auto *inventory = new InventoryWidget(&controller_);
  auto *relics = new RelicPlannerWidget(&controller_);
  auto *market = new MarketWidget(&controller_);
  auto *settings = new SettingsWidget(&controller_);
  wfgui::setCaptureTarget(pages_, "center-rail");
  wfgui::setCaptureTarget(foundry, "foundry");
  wfgui::setCaptureTarget(mastery, "mastery");
  wfgui::setCaptureTarget(inventory, "inventory");
  wfgui::setCaptureTarget(relics, "relic-planner");
  wfgui::setCaptureTarget(buildPlanner_, "build-planner");
  wfgui::setCaptureTarget(market, "market");
  wfgui::setCaptureTarget(settings, "settings");
  wfgui::setCaptureTarget(activityRail_, "right-rail");
  pages_->addWidget(foundry);
  pages_->addWidget(mastery);
  pages_->addWidget(inventory);
  pages_->addWidget(relics);
  pages_->addWidget(buildPlanner_);
  pages_->addWidget(market);
  pages_->addWidget(settings);
  pages_->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Expanding);
  pages_->setMinimumWidth(0);
  layout->addWidget(pages_, 1);
  layout->addWidget(activityRail_);
  windowLayout->addWidget(central, 1);
  setCentralWidget(windowRoot);

  constexpr std::array<Qt::Edges, 8> edges = {
      Qt::TopEdge,
      Qt::BottomEdge,
      Qt::LeftEdge,
      Qt::RightEdge,
      Qt::TopEdge | Qt::LeftEdge,
      Qt::TopEdge | Qt::RightEdge,
      Qt::BottomEdge | Qt::LeftEdge,
      Qt::BottomEdge | Qt::RightEdge,
  };
  for (Qt::Edges edge : edges) {
    resizeHandles_.append(new WindowResizeHandle(edge, this));
  }
  positionResizeHandles();

  connect(navigation_, &QButtonGroup::idClicked, this, &MainWindow::selectPage);
  connect(titleBar_, &TitleBarWidget::uiScaleDeltaRequested, this,
          &MainWindow::changeUiScale);
  connect(titleBar_, &TitleBarWidget::leftRailToggleRequested, this,
          &MainWindow::toggleLeftRail);
  connect(titleBar_, &TitleBarWidget::rightRailToggleRequested, this,
          &MainWindow::toggleRightRail);
  const auto openMarket = [this](const QString &item, const QString &side) {
    showMarketItem(item, side);
  };
  const auto showRelics = [this, relics](const QString &reward) {
    setPage("relic");
    relics->showReward(reward);
  };
  const auto showFoundry = [this, foundry](const QString &item) {
    setPage("foundry");
    foundry->showItem(item);
  };
  connect(foundry, &FoundryWidget::marketItemRequested, this, openMarket);
  connect(foundry, &FoundryWidget::relicRewardRequested, this, showRelics);
  connect(mastery, &MasteryPlannerWidget::marketItemRequested, this,
          openMarket);
  connect(mastery, &MasteryPlannerWidget::relicRewardRequested, this,
          showRelics);
  connect(mastery, &MasteryPlannerWidget::foundryItemRequested, this,
          showFoundry);
  connect(inventory, &InventoryWidget::marketItemRequested, this, openMarket);
  connect(inventory, &InventoryWidget::relicRewardRequested, this, showRelics);
  connect(relics, &RelicPlannerWidget::marketItemRequested, this, openMarket);
  connect(relics, &RelicPlannerWidget::foundryItemRequested, this, showFoundry);
  connect(market, &MarketWidget::marketItemRequested, this, openMarket);
  connect(marketDialog_, &MarketItemDialog::signInRequested, this, [this] {
    selectPage(MarketPage);
    navigation_->button(MarketPage)->setChecked(true);
  });
  connect(activityRail_, &ActivityRailWidget::signInRequested, this, [this] {
    selectPage(MarketPage);
    navigation_->button(MarketPage)->setChecked(true);
  });
  connect(activityRail_, &ActivityRailWidget::relicEraRequested, this,
          [this, relics](const QString &era) {
            setPage("relic");
            relics->showEra(era);
          });
}

void MainWindow::showMarketItem(const QString &item, const QString &side) {
  if (activityRail_->isVisible()) {
    activityRail_->showMarketItem(item, side);
  } else {
    marketDialog_->showItem(item, side);
  }
}

bool MainWindow::setActivityTab(const QString &tab) {
  return activityRail_->setTab(tab);
}

QWidget *MainWindow::screenshotTarget() {
  return marketDialog_->isVisible() ? static_cast<QWidget *>(marketDialog_)
                                    : this;
}

bool MainWindow::setPage(const QString &page) {
  const int index = pageNames().indexOf(page.toLower());
  if (index < 0) {
    return false;
  }
  selectPage(index);
  navigation_->button(index)->setChecked(true);
  return true;
}

bool MainWindow::setBuildPlannerMode(const QString &mode) {
  return buildPlanner_->setMode(mode);
}

void MainWindow::selectPage(int page) {
  pages_->setCurrentIndex(page);
  controller_.setActivePage(currentPageName());
  if (page == FoundryPage) {
    controller_.ensureFoundry();
  } else if (page == MasteryPage) {
    controller_.ensureMastery();
  } else if (page == InventoryPage) {
    controller_.ensureInventory();
  } else if (page == RelicPage) {
    controller_.ensureRelics();
  } else if (page == MarketPage) {
    controller_.ensureInventory();
    controller_.ensureMarket();
  } else if (page == SettingsPage) {
    controller_.refreshSourceAssetCache();
  }
}

void MainWindow::changeUiScale(int delta) {
  const int current = wfgui::configuredUiScalePercent();
  const int next = qBound(wfgui::MinimumUiScalePercent, current + delta,
                          wfgui::MaximumUiScalePercent);
  if (next == current) {
    return;
  }
  wfgui::setConfiguredUiScalePercent(next);
  const QStringList arguments = {"--page", currentPageName()};
  if (QProcess::startDetached(QCoreApplication::applicationFilePath(),
                              arguments, QDir::currentPath())) {
    QApplication::quit();
  } else {
    wfgui::setConfiguredUiScalePercent(current);
    qWarning("could not restart wfgui after UI scale change");
  }
}

void MainWindow::toggleLeftRail() {
  if (leftRailAnimating_) {
    return;
  }
  leftRailAnimating_ = true;
  leftRailCollapsed_ = !leftRailCollapsed_;
  titleBar_->setLeftRailCollapsed(leftRailCollapsed_);
  playerIdentity_->setVisible(!leftRailCollapsed_);
  for (int index = 0; index < navigationButtons_.size(); ++index) {
    QPushButton *button = navigationButtons_.at(index);
    button->setText(leftRailCollapsed_ ? QString()
                                       : navigationLabels_.at(index));
    button->setProperty("collapsed", leftRailCollapsed_);
    button->style()->unpolish(button);
    button->style()->polish(button);
  }
  animateRail(sidebar_, leftRailCollapsed_ ? 52 : 142, false,
              [this] { leftRailAnimating_ = false; });
}

void MainWindow::toggleRightRail() {
  if (width() < ActivityRailThreshold || rightRailAnimating_) {
    return;
  }
  rightRailAnimating_ = true;
  rightRailCollapsed_ = !rightRailCollapsed_;
  titleBar_->setRightRailCollapsed(rightRailCollapsed_);
  animateRail(activityRail_, rightRailCollapsed_ ? 0 : 400, rightRailCollapsed_,
              [this] { rightRailAnimating_ = false; });
}

void MainWindow::animateRail(QWidget *rail, int targetWidth, bool hideAfter,
                             const std::function<void()> &finished) {
  const int startWidth = rail->width();
  rail->setVisible(true);
  rail->setMinimumWidth(startWidth);
  rail->setMaximumWidth(startWidth);
  auto *group = new QParallelAnimationGroup(this);
  for (const QByteArray &property :
       {QByteArray("minimumWidth"), QByteArray("maximumWidth")}) {
    auto *animation = new QPropertyAnimation(rail, property, group);
    animation->setDuration(170);
    animation->setStartValue(startWidth);
    animation->setEndValue(targetWidth);
    animation->setEasingCurve(QEasingCurve::InOutCubic);
    group->addAnimation(animation);
  }
  connect(group, &QParallelAnimationGroup::finished, this,
          [rail, targetWidth, hideAfter, finished] {
            rail->setMinimumWidth(targetWidth);
            rail->setMaximumWidth(targetWidth);
            if (hideAfter) {
              rail->hide();
            }
            finished();
          });
  group->start(QAbstractAnimation::DeleteWhenStopped);
}

QString MainWindow::currentPageName() const {
  return pageNames().value(pages_->currentIndex(), "foundry");
}

void MainWindow::resizeEvent(QResizeEvent *event) {
  QMainWindow::resizeEvent(event);
  const bool rightRailAvailable =
      event->size().width() >= ActivityRailThreshold;
  titleBar_->setRightRailAvailable(rightRailAvailable);
  if (!rightRailCollapsed_) {
    activityRail_->setVisible(rightRailAvailable);
  }
  positionResizeHandles();
}

void MainWindow::changeEvent(QEvent *event) {
  QMainWindow::changeEvent(event);
  if (event->type() == QEvent::WindowStateChange) {
    titleBar_->setMaximized(isMaximized());
    positionResizeHandles();
  }
}

void MainWindow::positionResizeHandles() {
  if (resizeHandles_.size() != 8) {
    return;
  }
  const bool enabled = !isMaximized() && !isFullScreen();
  constexpr int edge = 5;
  constexpr int corner = 10;
  const int w = width();
  const int h = height();
  resizeHandles_.at(0)->setGeometry(corner, 0, qMax(0, w - 2 * corner), edge);
  resizeHandles_.at(1)->setGeometry(corner, h - edge, qMax(0, w - 2 * corner),
                                    edge);
  resizeHandles_.at(2)->setGeometry(0, corner, edge, qMax(0, h - 2 * corner));
  resizeHandles_.at(3)->setGeometry(w - edge, corner, edge,
                                    qMax(0, h - 2 * corner));
  resizeHandles_.at(4)->setGeometry(0, 0, corner, corner);
  resizeHandles_.at(5)->setGeometry(w - corner, 0, corner, corner);
  resizeHandles_.at(6)->setGeometry(0, h - corner, corner, corner);
  resizeHandles_.at(7)->setGeometry(w - corner, h - corner, corner, corner);
  for (QWidget *handle : resizeHandles_) {
    handle->setVisible(enabled);
    handle->raise();
  }
}
