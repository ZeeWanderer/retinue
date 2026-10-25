#include "build_equipment_widget.h"

#include <QAbstractItemModel>
#include <QComboBox>
#include <QEasingCurve>
#include <QFontMetrics>
#include <QFrame>
#include <QHBoxLayout>
#include <QIcon>
#include <QInputDialog>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QListView>
#include <QMenu>
#include <QPainter>
#include <QPushButton>
#include <QResizeEvent>
#include <QScrollArea>
#include <QSplitter>
#include <QToolButton>
#include <QVBoxLayout>
#include <QVariantAnimation>

#include "app_controller.h"
#include "build_equipment_model.h"
#include "build_group_model.h"
#include "build_topology_widget.h"

namespace {
QString displayClass(QString value) {
  value.replace('_', ' ');
  if (!value.isEmpty()) {
    value[0] = value[0].toUpper();
  }
  return value;
}

QString copyName(const QJsonObject &instance, int row) {
  const QString custom = instance.value("custom_name").toString();
  return custom.isEmpty() ? QString("Copy %1").arg(row + 1) : custom;
}

QString configName(const QJsonObject &config) {
  const int index = config.value("config_index").toInt();
  const QString name = config.value("name").toString();
  return name.isEmpty() ? QString("Configuration %1").arg(index + 1) : name;
}

QIcon tintedIcon(const QString &path) {
  const QPixmap source(path);
  QPixmap result(source.size());
  result.fill(Qt::transparent);
  QPainter painter(&result);
  painter.drawPixmap(0, 0, source);
  painter.setCompositionMode(QPainter::CompositionMode_SourceIn);
  painter.fillRect(result.rect(), QColor("#d8dbea"));
  return QIcon(result);
}
} // namespace

BuildEquipmentWidget::BuildEquipmentWidget(AppController *controller,
                                           QWidget *parent)
    : QWidget(parent), controller_(controller),
      equipment_(new BuildEquipmentFilterModel(this)), splitter_(new QSplitter),
      rail_(new QFrame), detail_(new QFrame), search_(new QLineEdit),
      category_(new QComboBox), list_(new QListView), empty_(new QLabel),
      collapse_(new QToolButton), railAnimation_(new QVariantAnimation(this)),
      back_(new QPushButton("Back to equipment")), title_(new QLabel),
      meta_(new QLabel), copy_(new QComboBox), config_(new QComboBox),
      topology_(new BuildTopologyWidget(controller)),
      createGroup_(new QPushButton("Create group")),
      captureConfig_(new QPushButton("Add current configuration")),
      state_(new QLabel) {
  equipment_->setSourceModel(controller_->buildEquipment());

  auto *layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(0);
  splitter_->setObjectName("buildEquipmentSplit");
  splitter_->setChildrenCollapsible(true);
  layout->addWidget(splitter_);

  rail_->setObjectName("buildPane");
  auto *railLayout = new QVBoxLayout(rail_);
  railLayout->setContentsMargins(8, 8, 8, 8);
  railLayout->setSpacing(7);
  search_->setPlaceholderText("Search equipment");
  search_->setClearButtonEnabled(true);
  railLayout->addWidget(search_);
  const QList<QPair<QString, QString>> categories = {
      {"All equipment", "all"},    {"Warframes", "warframe"},
      {"Primary", "primary"},      {"Secondary", "secondary"},
      {"Melee", "melee"},          {"Exalted", "exalted"},
      {"Archwing", "archwing"},    {"Archguns", "archgun"},
      {"Archmelee", "archmelee"},  {"Necramechs", "necramech"},
      {"Companions", "companion"}, {"Companion weapons", "companion_weapon"},
      {"K-Drives", "kdrive"},      {"Amps", "amp"},
      {"Parazon", "parazon"},
  };
  for (const auto &[label, value] : categories) {
    category_->addItem(label, value);
  }
  railLayout->addWidget(category_);
  list_->setObjectName("buildEquipmentList");
  list_->setModel(equipment_);
  list_->setSelectionMode(QAbstractItemView::SingleSelection);
  list_->setUniformItemSizes(true);
  list_->setSpacing(4);
  railLayout->addWidget(list_, 1);
  empty_->setObjectName("emptyState");
  empty_->setAlignment(Qt::AlignCenter);
  empty_->setWordWrap(true);
  railLayout->addWidget(empty_);
  splitter_->addWidget(rail_);

  detail_->setObjectName("buildPane");
  auto *detailLayout = new QVBoxLayout(detail_);
  detailLayout->setContentsMargins(12, 10, 12, 10);
  detailLayout->setSpacing(8);
  auto *detailHeader = new QHBoxLayout;
  back_->setObjectName("textAction");
  back_->setVisible(false);
  detailHeader->addWidget(back_);
  collapse_->setObjectName("compactTool");
  collapse_->setIcon(tintedIcon(":/resources/ui/panel-left.png"));
  collapse_->setIconSize({16, 16});
  collapse_->setToolTip("Collapse equipment list");
  detailHeader->addWidget(collapse_);
  title_->setObjectName("sectionTitle");
  detailHeader->addWidget(title_);
  detailHeader->addStretch();
  detailLayout->addLayout(detailHeader);
  meta_->setObjectName("secondaryText");
  detailLayout->addWidget(meta_);

  auto *selectors = new QHBoxLayout;
  selectors->addWidget(new QLabel("Copy"));
  selectors->addWidget(copy_, 1);
  selectors->addWidget(new QLabel("Configuration"));
  selectors->addWidget(config_, 1);
  detailLayout->addLayout(selectors);

  auto *scroll = new QScrollArea;
  scroll->setObjectName("buildTopologyScroll");
  scroll->setWidgetResizable(true);
  scroll->setFrameShape(QFrame::NoFrame);
  scroll->setWidget(topology_);
  detailLayout->addWidget(scroll, 1);

  state_->setObjectName("secondaryText");
  state_->setWordWrap(true);
  detailLayout->addWidget(state_);
  auto *actions = new QHBoxLayout;
  openGroup_ = new QPushButton("Open group");
  openGroup_->setObjectName("textAction");
  openGroup_->hide();
  actions->addWidget(openGroup_);
  connect(openGroup_, &QPushButton::clicked, this, [this] {
    emit groupRequested(addedGroupId_);
  });
  actions->addStretch();
  actions->addWidget(createGroup_);
  captureConfig_->setObjectName("primaryAction");
  actions->addWidget(captureConfig_);
  detailLayout->addLayout(actions);
  splitter_->addWidget(detail_);
  splitter_->setStretchFactor(0, 0);
  splitter_->setStretchFactor(1, 1);
  splitter_->setCollapsible(0, true);
  splitter_->setCollapsible(1, false);

  railAnimation_->setObjectName("buildRailAnimation");
  railAnimation_->setDuration(160);
  railAnimation_->setEasingCurve(QEasingCurve::OutCubic);
  connect(railAnimation_, &QVariantAnimation::valueChanged, this,
          [this](const QVariant &value) {
            const int railWidth = value.toInt();
            splitter_->setSizes({railWidth, qMax(400, width() - railWidth)});
          });
  connect(railAnimation_, &QVariantAnimation::finished, this, [this] {
    rail_->setVisible(!railCollapsed_);
    if (!railCollapsed_) {
      splitter_->setSizes(
          {preferredRailWidth(), qMax(400, width() - preferredRailWidth())});
    }
  });

  connect(search_, &QLineEdit::textChanged, equipment_,
          &BuildEquipmentFilterModel::setText);
  connect(category_, &QComboBox::currentIndexChanged, this, [this](int row) {
    equipment_->setCategory(category_->itemData(row).toString());
    restoreSelection();
  });
  connect(list_->selectionModel(), &QItemSelectionModel::currentChanged, this,
          [this](const QModelIndex &index) { selectEquipment(index); });
  connect(copy_, &QComboBox::currentIndexChanged, this,
          &BuildEquipmentWidget::selectCopy);
  connect(config_, &QComboBox::currentIndexChanged, this,
          &BuildEquipmentWidget::selectConfig);
  connect(back_, &QPushButton::clicked, this, [this] {
    narrowDetail_ = false;
    updateResponsiveLayout();
  });
  connect(collapse_, &QToolButton::clicked, this,
          [this] { setRailCollapsed(!railCollapsed_, true); });
  connect(createGroup_, &QPushButton::clicked, this,
          [this] { createGroup(false); });
  connect(captureConfig_, &QPushButton::clicked, this,
          &BuildEquipmentWidget::showGroupMenu);
  connect(controller_, &AppController::buildEquipmentStateChanged, this,
          [this] {
            restoreSelection();
            updateState();
            updateRailWidth();
          });
  connect(controller_->buildEquipment(), &QAbstractItemModel::modelReset, this,
          &BuildEquipmentWidget::updateRailWidth);
  connect(controller_, &AppController::buildGroupsStateChanged, this,
          &BuildEquipmentWidget::updateState);
  connect(controller_, &AppController::buildGroupRequestFinished, this,
          [this](const QJsonObject &request, const QJsonObject &group) {
            const QString op = request.value("op").toString();
            if (op == "build_group_create" && pendingCreate_) {
              pendingCreate_ = false;
              pendingGroupId_ = group.value("id").toString();
              if (pendingCapture_) {
                addConfig(group);
              } else {
                emit groupRequested(pendingGroupId_);
                pendingGroupId_.clear();
              }
            } else if (op == "build_group_add_config" &&
                       group.value("id").toString() == pendingGroupId_) {
              addedGroupId_ = pendingGroupId_;
              pendingGroupId_.clear();
              pendingCapture_ = false;
              openGroup_->setText("Open " + group.value("name").toString());
              openGroup_->show();
            }
            updateState();
          });
  connect(controller_, &AppController::buildGroupRequestFailed, this,
          [this](const QJsonObject &, const QString &) {
            pendingCreate_ = false;
            pendingCapture_ = false;
            pendingGroupId_.clear();
            updateState();
          });

  updateState();
  updateResponsiveLayout();
}

void BuildEquipmentWidget::selectDefinition(const QString &definitionId,
                                            const QString &instanceId) {
  selectedDefinitionId_ = definitionId;
  selectedInstanceId_ = instanceId;
  restoreSelection();
}

void BuildEquipmentWidget::resizeEvent(QResizeEvent *event) {
  QWidget::resizeEvent(event);
  updateResponsiveLayout();
}

void BuildEquipmentWidget::restoreSelection() {
  QModelIndex selected;
  for (int row = 0; row < equipment_->rowCount(); ++row) {
    const QModelIndex candidate = equipment_->index(row, 0);
    if (candidate.data(BuildEquipmentModel::IdRole).toString() ==
        selectedDefinitionId_) {
      selected = candidate;
      break;
    }
  }
  if (!selected.isValid() && equipment_->rowCount() > 0) {
    selected = equipment_->index(0, 0);
  }
  if (selected.isValid()) {
    list_->setCurrentIndex(selected);
    selectEquipment(selected);
  } else {
    selectedDefinitionId_.clear();
    selectedInstanceId_.clear();
    instance_ = {};
    title_->clear();
    meta_->clear();
    copy_->clear();
    config_->clear();
    topology_->clear();
  }
}

void BuildEquipmentWidget::selectEquipment(const QModelIndex &index) {
  if (!index.isValid()) {
    return;
  }
  selectedDefinitionId_ = index.data(BuildEquipmentModel::IdRole).toString();
  title_->setText(index.data(BuildEquipmentModel::NameRole).toString());
  meta_->setText(
      displayClass(index.data(BuildEquipmentModel::ClassRole).toString()));
  const QVariantList instances =
      index.data(BuildEquipmentModel::InstancesRole).toList();
  const QString previous = selectedInstanceId_;
  copy_->blockSignals(true);
  copy_->clear();
  int selected = 0;
  for (int row = 0; row < instances.size(); ++row) {
    const QJsonObject instance =
        QJsonObject::fromVariantMap(instances.at(row).toMap());
    copy_->addItem(copyName(instance, row), instance.toVariantMap());
    if (instance.value("instance_id").toString() == previous) {
      selected = row;
    }
  }
  copy_->setCurrentIndex(instances.isEmpty() ? -1 : selected);
  copy_->blockSignals(false);
  selectCopy(copy_->currentIndex());
  if (width() < 780) {
    narrowDetail_ = true;
    updateResponsiveLayout();
  }
}

void BuildEquipmentWidget::selectCopy(int index) {
  instance_ = index >= 0
                  ? QJsonObject::fromVariantMap(copy_->itemData(index).toMap())
                  : QJsonObject{};
  selectedInstanceId_ = instance_.value("instance_id").toString();
  config_->blockSignals(true);
  config_->clear();
  const QJsonArray active = instance_.value("active_config_indices").toArray();
  for (const QJsonValue &value : instance_.value("configs").toArray()) {
    const QJsonObject config = value.toObject();
    const int configIndex = config.value("config_index").toInt();
    QString label = configName(config);
    if (active.contains(configIndex)) {
      label += " (active)";
    }
    config_->addItem(label, configIndex);
  }
  config_->setCurrentIndex(config_->count() > 0 ? 0 : -1);
  config_->blockSignals(false);
  selectConfig(config_->currentIndex());
  updateState();
}

void BuildEquipmentWidget::selectConfig(int index) {
  openGroup_->hide();
  if (instance_.isEmpty() || index < 0) {
    topology_->clear();
    return;
  }
  topology_->setPlayerInstance(instance_, config_->itemData(index).toInt());
  updateState();
}

void BuildEquipmentWidget::updateState() {
  const bool loaded = controller_->buildEquipmentLoaded();
  const bool noEquipment = equipment_->rowCount() == 0;
  empty_->setText(loaded ? "No configurable player equipment cached."
                         : QString());
  empty_->setVisible(loaded && noEquipment);
  list_->setVisible(!noEquipment);
  const bool hasInstance = !instance_.isEmpty();
  const bool hasConfig = hasInstance && config_->currentIndex() >= 0;
  createGroup_->setEnabled(hasInstance && !controller_->buildGroupsLoading());
  captureConfig_->setEnabled(hasConfig && !controller_->buildGroupsLoading());
  QStringList details;
  if (hasInstance) {
    details.append(
        QString("%1 Forma").arg(instance_.value("forma_count").toInt()));
    details.append(QString("%1 saved configuration%2")
                       .arg(config_->count())
                       .arg(config_->count() == 1 ? "" : "s"));
  }
  if (controller_->buildGroupsLoading()) {
    details.append("Updating groups...");
  }
  state_->setText(details.join("  ·  "));
}

void BuildEquipmentWidget::updateResponsiveLayout() {
  const bool narrow = width() < 780;
  if (narrow) {
    railAnimation_->stop();
    collapse_->setVisible(false);
    back_->setVisible(narrowDetail_);
    rail_->setVisible(!narrowDetail_);
    detail_->setVisible(narrowDetail_);
  } else {
    back_->setVisible(false);
    collapse_->setVisible(true);
    detail_->setVisible(true);
    updateCollapseControl();
    if (railAnimation_->state() != QAbstractAnimation::Running) {
      rail_->setVisible(!railCollapsed_);
      updateRailWidth();
    }
  }
}

void BuildEquipmentWidget::updateRailWidth() {
  if (width() < 780 || railCollapsed_ ||
      railAnimation_->state() == QAbstractAnimation::Running) {
    return;
  }
  const int railWidth = preferredRailWidth();
  splitter_->setSizes({railWidth, qMax(400, width() - railWidth)});
}

int BuildEquipmentWidget::preferredRailWidth() const {
  QFontMetrics metrics(list_->font());
  int longest = 0;
  for (int row = 0; row < equipment_->rowCount(); ++row) {
    longest = qMax(longest, metrics.horizontalAdvance(
                                equipment_->index(row, 0).data().toString()));
  }
  return qBound(220, longest + 72, 360);
}

void BuildEquipmentWidget::setRailCollapsed(bool collapsed, bool animated) {
  if (width() < 780) {
    railCollapsed_ = collapsed;
    updateResponsiveLayout();
    return;
  }
  railAnimation_->stop();
  railCollapsed_ = collapsed;
  updateCollapseControl();
  rail_->show();
  detail_->show();
  const int start =
      splitter_->sizes().value(0, collapsed ? preferredRailWidth() : 0);
  const int end = collapsed ? 0 : preferredRailWidth();
  if (!animated || !isVisible() || start == end) {
    splitter_->setSizes({end, qMax(400, width() - end)});
    rail_->setVisible(!collapsed);
    return;
  }
  railAnimation_->setStartValue(start);
  railAnimation_->setEndValue(end);
  railAnimation_->start();
}

void BuildEquipmentWidget::updateCollapseControl() {
  collapse_->setIcon(tintedIcon(railCollapsed_
                                    ? ":/resources/ui/panel-right.png"
                                    : ":/resources/ui/panel-left.png"));
  collapse_->setToolTip(railCollapsed_ ? "Show equipment list"
                                       : "Collapse equipment list");
}

void BuildEquipmentWidget::showGroupMenu() {
  if (instance_.isEmpty() || config_->currentIndex() < 0) {
    return;
  }
  controller_->ensureBuildGroups();
  QMenu menu(this);
  QAbstractItemModel *groups = controller_->buildGroups();
  for (int row = 0; row < groups->rowCount(); ++row) {
    const QModelIndex index = groups->index(row, 0);
    if (index.data(BuildGroupModel::DefinitionIdRole).toString() !=
        selectedDefinitionId_) {
      continue;
    }
    const QString target =
        index.data(BuildGroupModel::InstanceIdRole).toString();
    if (!target.isEmpty() && target != selectedInstanceId_) {
      continue;
    }
    QAction *action = menu.addAction(index.data().toString());
    action->setData(index.data(BuildGroupModel::RawRole));
  }
  if (!menu.actions().isEmpty()) {
    menu.addSeparator();
  }
  QAction *create = menu.addAction("New group...");
  QAction *selected = menu.exec(
      captureConfig_->mapToGlobal(QPoint(0, captureConfig_->height())));
  if (!selected) {
    return;
  }
  if (selected == create) {
    createGroup(true);
  } else {
    addConfig(QJsonObject::fromVariantMap(selected->data().toMap()));
  }
}

void BuildEquipmentWidget::createGroup(bool captureConfig) {
  if (selectedDefinitionId_.isEmpty() || selectedInstanceId_.isEmpty()) {
    return;
  }
  bool accepted = false;
  const QString name =
      QInputDialog::getText(this, "New build group", "Name", QLineEdit::Normal,
                            title_->text() + " builds", &accepted);
  if (!accepted || name.trimmed().isEmpty()) {
    return;
  }
  pendingCreate_ = true;
  pendingCapture_ = captureConfig;
  controller_->createBuildGroup({{"name", name.trimmed()},
                                 {"definition_id", selectedDefinitionId_},
                                 {"instance_id", selectedInstanceId_}});
  updateState();
}

void BuildEquipmentWidget::addConfig(const QJsonObject &group) {
  const QString id = group.value("id").toString();
  if (id.isEmpty() || config_->currentIndex() < 0) {
    pendingCapture_ = false;
    return;
  }
  pendingGroupId_ = id;
  pendingCapture_ = true;
  controller_->addBuildConfigToGroup(id, group.value("revision").toInteger(),
                                     selectedInstanceId_,
                                     config_->currentData().toInt());
  updateState();
}
