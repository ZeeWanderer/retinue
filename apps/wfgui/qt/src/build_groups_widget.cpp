#include "build_groups_widget.h"

#include <QButtonGroup>
#include <QAbstractItemView>
#include <QCheckBox>
#include <QComboBox>
#include <QEvent>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QIcon>
#include <QInputDialog>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPixmap>
#include <QPushButton>
#include <QScrollArea>
#include <QScrollBar>
#include <QSignalBlocker>
#include <QStackedWidget>
#include <QStyle>
#include <QStyledItemDelegate>
#include <QStyleOptionViewItem>
#include <QToolButton>
#include <QVBoxLayout>

#include "app_controller.h"
#include "build_equipment_model.h"
#include "build_group_model.h"
#include "build_plan_widget.h"
#include "build_topology_widget.h"
#include "widget_capture.h"

namespace {
class BuildSelectionDelegate final : public QStyledItemDelegate {
public:
  explicit BuildSelectionDelegate(QComboBox *combo)
      : QStyledItemDelegate(combo), combo_(combo), check_(":/resources/market/wfgui-check.png") {
    QPixmap empty(16, 16);
    empty.fill(Qt::transparent);
    empty_ = QIcon(empty);
  }

  QSize sizeHint(const QStyleOptionViewItem &option, const QModelIndex &index) const override {
    return QStyledItemDelegate::sizeHint(option, index) + QSize(0, 8);
  }

protected:
  void initStyleOption(QStyleOptionViewItem *option, const QModelIndex &index) const override {
    QStyledItemDelegate::initStyleOption(option, index);
    const bool current = index.row() == combo_->currentIndex();
    option->features |= QStyleOptionViewItem::HasDecoration;
    option->icon = current ? check_ : empty_;
    option->decorationSize = QSize(16, 16);
    if (current) option->font.setWeight(QFont::Medium);
    option->fontMetrics = QFontMetrics(option->font);
    option->textElideMode = Qt::ElideMiddle;
  }

private:
  QComboBox *combo_;
  QIcon check_;
  QIcon empty_;
};
} // namespace

BuildGroupsWidget::BuildGroupsWidget(AppController *controller, QWidget *parent)
    : QWidget(parent), controller_(controller), pages_(new QStackedWidget),
      contentPages_(new QStackedWidget),
      editor_(new QWidget), emptyPage_(new QWidget), scroll_(new QScrollArea),
      actions_(new QWidget), groups_(new QComboBox),
      target_(new QComboBox), members_(new QComboBox), meta_(new QLabel),
      state_(new QLabel), emptyTitle_(new QLabel), emptyDescription_(new QLabel),
      emptyEquipment_(new QPushButton("Choose equipment")),
      emptyDiscover_(new QPushButton("Discover builds")),
      preserveSlots_(new QCheckBox("Keep exact mod slots")),
      allowOmni_(new QCheckBox("Allow new Omni Forma")),
      allowUmbral_(new QCheckBox("Allow new Umbral Forma")),
      plan_(new BuildPlanWidget), topology_(new BuildTopologyWidget(controller)),
      capacity_(new QLabel), notes_(new QLabel),
      original_(new QPushButton("Target build")), planned_(new QPushButton("Planned loadout")),
      remove_(new QPushButton("Remove build")), calculate_(new QPushButton("Calculate")),
      equipment_(new QPushButton("Add current configuration")), discover_(new QPushButton("Find builds")) {
  wfgui::setCaptureTarget(this, "build-planner.groups");
  auto *layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->addWidget(pages_);
  pages_->addWidget(editor_);
  pages_->addWidget(emptyPage_);
  editor_->setObjectName("buildGroupEditor");
  auto *editorLayout = new QVBoxLayout(editor_);
  editorLayout->setContentsMargins(0, 0, 0, 0);
  editorLayout->setSpacing(10);

  auto *header = new QHBoxLayout;
  header->addWidget(new QLabel("Group"));
  groups_->setObjectName("buildGroupSelect");
  groups_->setModel(controller_->buildGroups());
  groups_->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
  header->addWidget(groups_, 1);
  auto *rename = new QPushButton("Rename");
  rename->setObjectName("textAction");
  header->addWidget(rename);
  auto *deleteGroup = new QPushButton("Delete");
  deleteGroup->setProperty("destructive", true);
  header->addWidget(deleteGroup);
  editorLayout->addLayout(header);

  auto *targetRow = new QHBoxLayout;
  targetRow->addWidget(new QLabel("Owned copy"));
  target_->setObjectName("buildTargetCopy");
  targetRow->addWidget(target_, 1);
  meta_->setObjectName("secondaryText");
  targetRow->addWidget(meta_);
  calculate_->setObjectName("primaryAction");
  calculate_->setProperty("testId", "buildGroupCalculate");
  targetRow->addWidget(calculate_);
  editorLayout->addLayout(targetRow);

  auto *optionsButton = new QToolButton;
  optionsButton->setObjectName("buildOptionsToggle");
  optionsButton->setText("Planning options");
  optionsButton->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
  optionsButton->setArrowType(Qt::RightArrow);
  optionsButton->setCheckable(true);
  editorLayout->addWidget(optionsButton, 0, Qt::AlignLeft);
  auto *options = new QWidget;
  options->setObjectName("buildOptions");
  auto *optionsLayout = new QHBoxLayout(options);
  optionsLayout->setContentsMargins(0, 0, 0, 0);
  preserveSlots_->setObjectName("buildKeepExactSlots");
  preserveSlots_->setToolTip("Disable mod rearrangement. Elemental combinations are preserved either way.");
  for (auto *option : {preserveSlots_, allowOmni_, allowUmbral_}) {
    optionsLayout->addWidget(option);
    connect(option, &QCheckBox::toggled, this, &BuildGroupsWidget::saveOptions);
  }
  optionsLayout->addStretch();
  options->hide();
  editorLayout->addWidget(options);
  connect(optionsButton, &QToolButton::toggled, this, [options, optionsButton](bool visible) {
    options->setVisible(visible);
    optionsButton->setArrowType(visible ? Qt::DownArrow : Qt::RightArrow);
  });

  state_->setObjectName("buildGroupState");
  state_->setWordWrap(true);
  editorLayout->addWidget(state_);
  scroll_->setObjectName("buildRevisionScroll");
  scroll_->setWidgetResizable(true);
  scroll_->setFrameShape(QFrame::NoFrame);
  auto *body = new QWidget;
  auto *bodyLayout = new QVBoxLayout(body);
  bodyLayout->setContentsMargins(0, 12, 8, 12);
  bodyLayout->setSpacing(12);
  bodyLayout->setSizeConstraint(QLayout::SetMinAndMaxSize);
  bodyLayout->addWidget(plan_);
  auto *memberRow = new QHBoxLayout;
  memberRow->addWidget(new QLabel("Build"));
  members_->setObjectName("buildGroupMembers");
  members_->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
  members_->setMinimumContentsLength(15);
  members_->setSizeAdjustPolicy(QComboBox::AdjustToMinimumContentsLengthWithIcon);
  members_->setItemDelegate(new BuildSelectionDelegate(members_));
  memberRow->addWidget(members_, 1);
  remove_->setProperty("destructive", true);
  memberRow->addWidget(remove_);
  bodyLayout->addLayout(memberRow);
  auto *viewRow = new QHBoxLayout;
  auto *viewMode = new QButtonGroup(this);
  for (auto *button : {original_, planned_}) {
    button->setObjectName("filterChip");
    button->setCheckable(true);
    viewMode->addButton(button);
    viewRow->addWidget(button);
    connect(button, &QPushButton::clicked, this, &BuildGroupsWidget::showMember);
  }
  original_->setProperty("testId", "buildOriginal");
  planned_->setProperty("testId", "buildPlanned");
  original_->setChecked(true);
  viewRow->addStretch();
  capacity_->setObjectName("buildCapacity");
  viewRow->addWidget(capacity_);
  bodyLayout->addLayout(viewRow);
  bodyLayout->addWidget(topology_);
  notes_->setObjectName("buildNote");
  notes_->setWordWrap(true);
  notes_->setTextFormat(Qt::MarkdownText);
  notes_->setTextInteractionFlags(Qt::TextBrowserInteraction);
  notes_->setOpenExternalLinks(true);
  bodyLayout->addWidget(notes_);
  bodyLayout->addStretch();
  scroll_->setWidget(body);
  contentPages_->addWidget(scroll_);
  auto *emptyMembers = new QWidget;
  emptyMembers->setObjectName("buildGroupEmptyMembers");
  auto *emptyMembersLayout = new QVBoxLayout(emptyMembers);
  emptyMembersLayout->addStretch();
  auto *emptyMembersTitle = new QLabel("No target builds");
  emptyMembersTitle->setObjectName("sectionTitle");
  emptyMembersTitle->setAlignment(Qt::AlignCenter);
  emptyMembersLayout->addWidget(emptyMembersTitle);
  auto *emptyMembersDescription = new QLabel("Add an Overframe build or capture a configuration from Equipment.");
  emptyMembersDescription->setObjectName("secondaryText");
  emptyMembersDescription->setWordWrap(true);
  emptyMembersDescription->setAlignment(Qt::AlignCenter);
  emptyMembersLayout->addWidget(emptyMembersDescription);
  emptyMembersLayout->addStretch();
  contentPages_->addWidget(emptyMembers);
  auto *workspace = new QWidget;
  auto *layers = new QGridLayout(workspace);
  layers->setContentsMargins(0, 0, 0, 0);
  layers->addWidget(contentPages_, 0, 0);
  auto *edge = new QFrame;
  edge->setObjectName("buildScrollEdge");
  edge->setFixedHeight(8);
  edge->setAttribute(Qt::WA_TransparentForMouseEvents);
  layers->addWidget(edge, 0, 0, Qt::AlignTop);
  edge->hide();
  connect(scroll_->verticalScrollBar(), &QScrollBar::valueChanged, this, [edge](int value) {
    edge->setVisible(value > 0);
  });
  connect(contentPages_, &QStackedWidget::currentChanged, this, [this, edge](int index) {
    edge->setVisible(index == 0 && scroll_->verticalScrollBar()->value() > 0);
  });
  actions_->setObjectName("buildGroupActions");
  actions_->setAttribute(Qt::WA_StyledBackground);
  auto *actions = new QHBoxLayout(actions_);
  actions->setContentsMargins(8, 8, 8, 8);
  actions->addWidget(equipment_);
  actions->addWidget(discover_);
  auto *actionInset = new QHBoxLayout;
  actionInset->setContentsMargins(0, 0, style()->pixelMetric(QStyle::PM_ScrollBarExtent) + 8, 8);
  actionInset->addWidget(actions_);
  layers->addLayout(actionInset, 0, 0, Qt::AlignBottom | Qt::AlignRight);
  actions_->installEventFilter(this);
  editorLayout->addWidget(workspace, 1);

  emptyPage_->setObjectName("buildGroupEmpty");
  auto *emptyLayout = new QVBoxLayout(emptyPage_);
  emptyLayout->setContentsMargins(24, 24, 24, 24);
  emptyLayout->addStretch();
  emptyTitle_->setObjectName("sectionTitle");
  emptyTitle_->setProperty("testId", "buildGroupEmptyTitle");
  emptyTitle_->setAlignment(Qt::AlignCenter);
  emptyLayout->addWidget(emptyTitle_);
  emptyDescription_->setObjectName("secondaryText");
  emptyDescription_->setAlignment(Qt::AlignCenter);
  emptyDescription_->setWordWrap(true);
  emptyLayout->addWidget(emptyDescription_);
  auto *emptyActions = new QHBoxLayout;
  emptyActions->addStretch();
  emptyEquipment_->setObjectName("primaryAction");
  emptyEquipment_->setProperty("testId", "buildGroupEmptyEquipment");
  emptyActions->addWidget(emptyEquipment_);
  emptyDiscover_->setObjectName("textAction");
  emptyDiscover_->setProperty("testId", "buildGroupEmptyDiscover");
  emptyActions->addWidget(emptyDiscover_);
  emptyActions->addStretch();
  emptyLayout->addLayout(emptyActions);
  emptyLayout->addStretch();

  connect(groups_, &QComboBox::activated, this, [this](int index) {
    selectGroup(groups_->itemData(index, BuildGroupModel::IdRole).toString());
  });
  connect(target_, &QComboBox::activated, this, [this] {
    controller_->updateBuildGroup(selectedId_, group_.value("revision").toInteger(),
                                   {{"instance_id", target_->currentData().toString()}});
  });
  connect(members_, &QComboBox::currentIndexChanged, this, [this] {
    if (!refreshing_) showMember();
  });
  connect(calculate_, &QPushButton::clicked, this, &BuildGroupsWidget::calculate);
  connect(rename, &QPushButton::clicked, this, [this] {
    bool ok = false;
    const QString name = QInputDialog::getText(this, "Rename group", "Name", QLineEdit::Normal,
                                              group_.value("name").toString(), &ok).trimmed();
    if (ok && !name.isEmpty()) {
      controller_->updateBuildGroup(selectedId_, group_.value("revision").toInteger(), {{"name", name}});
    }
  });
  connect(deleteGroup, &QPushButton::clicked, this, [this] {
    if (QMessageBox::question(this, "Delete build group", "Delete " + group_.value("name").toString() + "?") == QMessageBox::Yes) {
      controller_->deleteBuildGroup(selectedId_, group_.value("revision").toInteger());
    }
  });
  connect(remove_, &QPushButton::clicked, this, [this] {
    controller_->removeBuildGroupMember(selectedId_, group_.value("revision").toInteger(), members_->currentData().toString());
  });
  connect(equipment_, &QPushButton::clicked, this, [this] {
    emit equipmentRequested(group_.value("definition_id").toString(), group_.value("instance_id").toString());
  });
  connect(discover_, &QPushButton::clicked, this, [this] {
    emit discoverRequested(group_.value("definition_id").toString());
  });
  connect(emptyEquipment_, &QPushButton::clicked, this, [this] {
    if (!controller_->buildGroupsError().isEmpty()) controller_->refreshBuildGroups();
    else emit equipmentRequested({}, {});
  });
  connect(emptyDiscover_, &QPushButton::clicked, this, [this] { emit discoverRequested({}); });
  connect(controller_, &AppController::buildGroupsStateChanged, this, &BuildGroupsWidget::refresh);
  connect(controller_, &AppController::buildEquipmentStateChanged, this, &BuildGroupsWidget::refresh);
  connect(controller_, &AppController::buildGroupChanged, this, [this](const QString &action, const QJsonObject &group) {
    if (group.value("id").toString() == selectedId_ && action == "planned") {
      planning_ = false;
      planned_->setChecked(true);
    }
    refresh();
  });
  connect(controller_, &AppController::buildGroupRequestFinished, this, [this](const QJsonObject &request, const QJsonObject &) {
    if (request.value("op").toString() == "build_group_plan" && request.value("group_id").toString() == selectedId_) {
      planning_ = false;
      planned_->setChecked(true);
    }
    refresh();
  });
  connect(controller_, &AppController::buildGroupRequestFailed, this, [this](const QJsonObject &request, const QString &) {
    if (request.value("op").toString() == "build_group_plan" && request.value("group_id").toString() == selectedId_) planning_ = false;
    refresh();
  });
  refresh();
}

bool BuildGroupsWidget::eventFilter(QObject *watched, QEvent *event) {
  if (watched == actions_ && event->type() == QEvent::Resize) {
    scroll_->widget()->layout()->setContentsMargins(0, 12, 8, actions_->height() + 20);
  }
  return QWidget::eventFilter(watched, event);
}

void BuildGroupsWidget::selectGroup(const QString &id) {
  if (id != selectedId_) {
    planning_ = false;
    original_->setChecked(true);
    rendered_ = {};
  }
  selectedId_ = id;
  refresh();
  if (!id.isEmpty()) controller_->requestBuildGroup(id);
}

void BuildGroupsWidget::refresh() {
  if (refreshing_) return;
  refreshing_ = true;
  const bool loaded = controller_->buildGroupsLoaded();
  const QString error = controller_->buildGroupsError();
  const bool empty = groups_->count() == 0;
  pages_->setCurrentWidget(empty ? emptyPage_ : editor_);
  emptyTitle_->setText(!error.isEmpty() ? "Could not load build groups" : loaded ? "No build groups" : "Loading build groups");
  emptyDescription_->setText(!error.isEmpty() ? error : loaded ? "Choose equipment and add the builds you want it to support." : QString());
  emptyEquipment_->setText(error.isEmpty() ? "Choose equipment" : "Retry");
  emptyEquipment_->setVisible(loaded || !error.isEmpty());
  emptyDiscover_->setVisible(loaded && error.isEmpty());
  if (empty) {
    group_ = {};
    selectedId_.clear();
    refreshing_ = false;
    return;
  }
  const int found = groups_->findData(selectedId_, BuildGroupModel::IdRole);
  groups_->setCurrentIndex(found < 0 ? 0 : found);
  const QString id = groups_->currentData(BuildGroupModel::IdRole).toString();
  const bool changed = id != selectedId_;
  selectedId_ = id;
  group_ = controller_->buildGroup(id);
  const auto baseline = group_.value("baseline").toObject();
  const auto result = group_.value("plan_result").toObject();
  const bool ready = result.value("status").toString() == "ready";
  refreshTargets();
  meta_->setText(baseline.isEmpty() ? QString() : QString("%1 Forma installed").arg(baseline.value("forma_count").toInt()));
  const auto options = group_.value("options").toObject();
  preserveSlots_->setChecked(options.value("preserve_source_slots").toBool(false));
  allowOmni_->setChecked(options.value("allow_omni").toBool());
  allowUmbral_->setChecked(options.value("allow_umbral_forma").toBool());
  const bool busy = controller_->buildGroupsLoading();
  for (auto *option : {preserveSlots_, allowOmni_, allowUmbral_}) option->setEnabled(!busy && !planning_);
  target_->setEnabled(!busy && !planning_ && target_->count() > 0);
  const QString selected = members_->currentData().toString();
  members_->clear();
  bool needDetails = false;
  for (const auto &value : group_.value("members").toArray()) {
    const auto member = value.toObject();
    members_->addItem(member.value("name").toString(), member.value("id").toString());
    members_->setItemData(members_->count() - 1, member.value("name").toString(), Qt::ToolTipRole);
    needDetails |= !member.value("snapshot").isObject();
  }
  const int memberIndex = members_->findData(selected);
  members_->setCurrentIndex(memberIndex < 0 ? 0 : memberIndex);
  members_->setEnabled(members_->count() > 0);
  contentPages_->setCurrentIndex(members_->count() > 0 ? 0 : 1);
  remove_->setEnabled(members_->count() > 0 && !busy && !planning_);
  equipment_->setEnabled(!group_.value("instance_id").toString().isEmpty());
  calculate_->setEnabled(!baseline.isEmpty() && members_->count() > 0 && !busy && !planning_ && !needDetails);
  calculate_->setText(planning_ ? "Calculating..." : ready ? "Recalculate" : "Calculate");
  planned_->setEnabled(ready);
  original_->setEnabled(members_->count() > 0);
  if (!ready) original_->setChecked(true);
  QString message;
  if (error.contains("unknown_mod_elements")) {
    message = "Elemental effects could not be resolved for a target mod. Enable Keep exact mod slots to plan without rearranging it.";
    findChild<QToolButton *>("buildOptionsToggle")->setChecked(true);
  }
  else if (!error.isEmpty()) message = error;
  else if (baseline.isEmpty()) message = "Choose an owned copy to plan its polarities.";
  else if (members_->count() == 0) message = {};
  else if (planning_) message = "Calculating changes for every build in this group...";
  else if (result.value("status").toString() == "blocked") {
    const QString reason = result.value("reason").toString();
    if (reason == "no_plan") {
      message = "These builds cannot share a layout under the current planning options.";
      if (preserveSlots_->isChecked()) message += " Uncheck Keep exact mod slots to allow rearrangement without changing elemental combinations.";
      else if (!allowOmni_->isChecked()) message += " Try allowing Omni Forma.";
    } else if (reason == "search_budget_exhausted") {
      message = "The search limit was reached. Reduce the group or relax its planning options.";
    } else message = "Plan blocked: " + reason;
    findChild<QToolButton *>("buildOptionsToggle")->setChecked(true);
  }
  else if (!ready) message = "Calculate to see polarity changes and the resulting loadouts.";
  state_->setText(message);
  state_->setVisible(!message.isEmpty());
  plan_->setVisible(!baseline.isEmpty() && members_->count() > 0);
  plan_->setPlan(baseline, result);
  refreshing_ = false;
  showMember();
  if (changed || needDetails) controller_->requestBuildGroup(selectedId_);
}

void BuildGroupsWidget::refreshTargets() {
  const QSignalBlocker blocker(target_);
  target_->clear();
  const QString id = group_.value("instance_id").toString();
  auto *model = controller_->buildEquipment();
  for (int row = 0; row < model->rowCount(); ++row) {
    const auto index = model->index(row, 0);
    if (index.data(BuildEquipmentModel::IdRole).toString() != group_.value("definition_id").toString()) continue;
    for (const auto &value : index.data(BuildEquipmentModel::InstancesRole).toList()) {
      const auto instance = value.toMap();
      const QString copyId = instance.value("instance_id").toString();
      QString label = index.data(BuildEquipmentModel::NameRole).toString();
      const QString custom = instance.value("custom_name").toString();
      if (!custom.isEmpty()) label = custom;
      label += QString(" (%1)").arg(copyId.right(6));
      target_->addItem(label, copyId);
    }
  }
  int selected = target_->findData(id);
  if (selected < 0) {
    target_->insertItem(0, id.isEmpty() ? "Choose a copy" : "Copy " + id.right(6), id);
    selected = 0;
  }
  target_->setCurrentIndex(selected);
}

void BuildGroupsWidget::showMember() {
  QJsonObject member;
  for (const auto &value : group_.value("members").toArray()) {
    if (value.toObject().value("id").toString() == members_->currentData().toString()) member = value.toObject();
  }
  const auto baseline = group_.value("baseline").toObject();
  const auto snapshot = member.value("snapshot").toObject();
  const auto result = group_.value("plan_result").toObject();
  QJsonObject loadout;
  if (planned_->isChecked()) {
    for (const auto &value : result.value("builds").toArray()) {
      if (value.toObject().value("member_id").toString() == member.value("id").toString()) loadout = value.toObject();
    }
  }
  capacity_->setText(loadout.isEmpty() ? QString() : QString("%1 / %2 capacity used · %3 free")
      .arg(loadout.value("drain").toInt()).arg(loadout.value("capacity").toInt()).arg(loadout.value("remaining_capacity").toInt()));
  QJsonObject render{{"snapshot", snapshot}, {"baseline", baseline}, {"loadout", loadout}};
  if (render == rendered_) return;
  rendered_ = render;
  topology_->setVisible(!snapshot.isEmpty());
  if (!loadout.isEmpty()) {
    auto planned = baseline;
    planned.insert("effective_polarities", result.value("final_polarities"));
    planned.insert("config", loadout);
    topology_->setPlayerSnapshot(planned);
  } else if (snapshot.isEmpty()) {
    topology_->clear();
  } else if (member.value("kind").toString() == "player_config") {
    auto current = baseline;
    current.insert("config", snapshot.value("config"));
    topology_->setPlayerSnapshot(current);
  } else {
    topology_->setSourceRevision(snapshot, baseline);
  }
  const QString note = snapshot.value("metadata").toObject().value("description").toString();
  notes_->setText(note);
  notes_->setVisible(!note.isEmpty());
}

void BuildGroupsWidget::saveOptions() {
  if (refreshing_ || group_.isEmpty()) return;
  controller_->updateBuildGroup(selectedId_, group_.value("revision").toInteger(),
      {{"options", QJsonObject{{"preserve_source_slots", preserveSlots_->isChecked()},
                               {"allow_omni", allowOmni_->isChecked()},
                               {"allow_umbral_forma", allowUmbral_->isChecked()}}}});
}

void BuildGroupsWidget::calculate() {
  if (group_.isEmpty() || planning_) return;
  planning_ = true;
  refresh();
  controller_->planBuildGroup(selectedId_, group_.value("revision").toInteger());
}
