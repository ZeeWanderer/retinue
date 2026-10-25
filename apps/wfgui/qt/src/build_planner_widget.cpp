#include "build_planner_widget.h"

#include <QButtonGroup>
#include <QHBoxLayout>
#include <QJsonObject>
#include <QLabel>
#include <QPushButton>
#include <QShowEvent>
#include <QStackedWidget>
#include <QStyle>
#include <QVBoxLayout>

#include "animated_progress_bar.h"
#include "app_controller.h"
#include "build_discover_widget.h"
#include "build_equipment_widget.h"
#include "build_groups_widget.h"
#include "overframe_login.h"

BuildPlannerWidget::BuildPlannerWidget(AppController *controller,
                                       QWidget *parent)
    : QWidget(parent), controller_(controller), login_(new OverframeLogin(this)),
      accountName_(new QLabel), error_(new QLabel),
      signIn_(new QPushButton("Sign in to Overframe")),
      signOut_(new QPushButton("Sign out")),
      groupsMode_(new QPushButton("Groups")),
      equipmentMode_(new QPushButton("Equipment")),
      discoverMode_(new QPushButton("Discover")),
      progress_(new AnimatedProgressBar), modes_(new QStackedWidget),
      groups_(new BuildGroupsWidget(controller)),
      equipment_(new BuildEquipmentWidget(controller)),
      discover_(new BuildDiscoverWidget(controller)) {
  setObjectName("page");
  auto *layout = new QVBoxLayout(this);
  layout->setContentsMargins(20, 10, 20, 0);
  layout->setSpacing(8);

  auto *modeRow = new QHBoxLayout;
  auto *construction = new QLabel;
  construction->setObjectName("constructionBadge");
  construction->setPixmap(
      style()->standardIcon(QStyle::SP_MessageBoxWarning).pixmap(18, 18));
  construction->setToolTip("Under construction");
  modeRow->addWidget(construction);
  auto *modeGroup = new QButtonGroup(this);
  modeGroup->setExclusive(true);
  for (QPushButton *button : {groupsMode_, equipmentMode_, discoverMode_}) {
    button->setObjectName("filterChip");
    button->setCheckable(true);
    modeGroup->addButton(button);
    modeRow->addWidget(button);
  }
  groupsMode_->setChecked(true);
  modeRow->addStretch();
  accountName_->setObjectName("secondaryText");
  modeRow->addWidget(accountName_);
  signIn_->setObjectName("textAction");
  signOut_->setObjectName("textAction");
  modeRow->addWidget(signIn_);
  modeRow->addWidget(signOut_);
  layout->addLayout(modeRow);

  progress_->setObjectName("priceProgress");
  progress_->setRange(0, 0);
  progress_->setTextVisible(false);
  progress_->setFixedHeight(2);
  layout->addWidget(progress_);
  error_->setObjectName("pageError");
  error_->setWordWrap(true);
  layout->addWidget(error_);

  modes_->setObjectName("buildModes");
  modes_->addWidget(groups_);
  modes_->addWidget(equipment_);
  modes_->addWidget(discover_);
  layout->addWidget(modes_, 1);

  connect(groupsMode_, &QPushButton::clicked, this, [this] { showGroups(); });
  connect(equipmentMode_, &QPushButton::clicked, this,
          [this] { showEquipment(); });
  connect(discoverMode_, &QPushButton::clicked, this,
          [this] { showDiscover(); });
  connect(signIn_, &QPushButton::clicked, this, [this] {
    loginError_.clear();
    login_->start();
    updateState();
  });
  connect(signOut_, &QPushButton::clicked, controller_,
          &AppController::overframeLogout);
  connect(login_, &OverframeLogin::cookiesReady, this,
          [this](const QJsonArray &cookies) {
            loginError_.clear();
            controller_->importOverframeSession(cookies);
            updateState();
          });
  connect(login_, &OverframeLogin::failed, this,
          [this](const QString &message) {
            loginError_ = message;
            updateState();
          });
  connect(login_, &OverframeLogin::activeChanged, this,
          &BuildPlannerWidget::updateState);
  connect(controller_, &AppController::overframeAccountChanged, this,
          &BuildPlannerWidget::updateState);
  connect(controller_, &AppController::buildEquipmentStateChanged, this,
          &BuildPlannerWidget::updateState);
  connect(controller_, &AppController::buildGroupsStateChanged, this,
          &BuildPlannerWidget::updateState);
  connect(controller_, &AppController::buildSourceItemsStateChanged, this,
          &BuildPlannerWidget::updateState);
  connect(controller_, &AppController::sourceBuildsStateChanged, this,
          &BuildPlannerWidget::updateState);
  connect(groups_, &BuildGroupsWidget::equipmentRequested, this,
          &BuildPlannerWidget::showEquipment);
  connect(groups_, &BuildGroupsWidget::discoverRequested, this,
          &BuildPlannerWidget::showDiscover);
  connect(equipment_, &BuildEquipmentWidget::groupRequested, this,
          &BuildPlannerWidget::showGroups);
  connect(discover_, &BuildDiscoverWidget::groupRequested, this,
          &BuildPlannerWidget::showGroups);
  updateState();
}

bool BuildPlannerWidget::setMode(const QString &mode) {
  const QString normalized = mode.trimmed().toLower();
  if (normalized == "groups" || normalized == "group") {
    showGroups();
    return true;
  }
  if (normalized == "equipment" || normalized == "my-equipment") {
    showEquipment();
    return true;
  }
  if (normalized == "discover") {
    showDiscover();
    return true;
  }
  return false;
}

void BuildPlannerWidget::showEvent(QShowEvent *event) {
  QWidget::showEvent(event);
  controller_->ensureBuildEquipment();
  controller_->ensureBuildGroups();
  controller_->refreshOverframeAccount();
}

void BuildPlannerWidget::updateState() {
  const QJsonObject account = controller_->overframeAccount();
  const bool authenticated = account.value("authenticated").toBool();
  const bool stale = account.value("stale").toBool();
  const bool accountBusy = controller_->overframeAccountBusy() || login_->active();
  const QJsonObject profile = account.value("profile").toObject();
  QString name = profile.value("username").toString();
  if (name.isEmpty()) {
    name = profile.value("name").toString();
  }
  accountName_->setText(name);
  accountName_->setVisible(authenticated && !name.isEmpty());
  signIn_->setText(stale ? "Sign in again" : "Sign in to Overframe");
  signIn_->setVisible(!authenticated);
  signIn_->setEnabled(!accountBusy);
  signOut_->setVisible(authenticated);
  signOut_->setEnabled(!accountBusy);

  progress_->setVisible(accountBusy || controller_->buildEquipmentLoading() ||
                        controller_->buildGroupsLoading() ||
                        controller_->buildSourceItemsLoading() ||
                        controller_->sourceBuildsLoading());
  QString message = loginError_.isEmpty()
                        ? controller_->overframeAccountError()
                        : loginError_;
  if (message.isEmpty()) {
    message = controller_->buildGroupsError();
  }
  if (message.isEmpty()) {
    message = controller_->buildEquipmentError();
  }
  if (message.isEmpty() && modes_->currentIndex() == 2) {
    message = controller_->buildSourceItemsError();
  }
  if (message.isEmpty() && modes_->currentIndex() == 2) {
    message = controller_->sourceBuildsError();
  }
  error_->setText(message);
  error_->setVisible(!message.isEmpty());
}

void BuildPlannerWidget::showGroups(const QString &groupId) {
  groupsMode_->setChecked(true);
  modes_->setCurrentWidget(groups_);
  controller_->ensureBuildGroups();
  if (!groupId.isEmpty()) {
    groups_->selectGroup(groupId);
  }
  updateState();
}

void BuildPlannerWidget::showEquipment(const QString &definitionId,
                                       const QString &instanceId) {
  equipmentMode_->setChecked(true);
  modes_->setCurrentWidget(equipment_);
  controller_->ensureBuildEquipment();
  if (!definitionId.isEmpty()) {
    equipment_->selectDefinition(definitionId, instanceId);
  }
  updateState();
}

void BuildPlannerWidget::showDiscover(const QString &definitionId) {
  discoverMode_->setChecked(true);
  modes_->setCurrentWidget(discover_);
  discover_->ensureLoaded();
  if (!definitionId.isEmpty()) {
    discover_->selectItem(definitionId);
  }
  updateState();
}
