#include "build_discover_widget.h"

#include <QAbstractItemModel>
#include <QComboBox>
#include <QDateTime>
#include <QFrame>
#include <QHBoxLayout>
#include <QInputDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QListView>
#include <QMenu>
#include <QPushButton>
#include <QScrollArea>
#include <QSplitter>
#include <QStandardItemModel>
#include <QTimer>
#include <QVBoxLayout>

#include "app_controller.h"
#include "build_group_model.h"
#include "build_source_model.h"
#include "build_topology_widget.h"

namespace {
QString buildDate(const QString &value) {
  const QDateTime date = QDateTime::fromString(value, Qt::ISODate);
  return date.isValid() ? date.toLocalTime().date().toString(Qt::ISODate)
                        : QString();
}
} // namespace

BuildDiscoverWidget::BuildDiscoverWidget(AppController *controller,
                                         QWidget *parent)
    : QWidget(parent), controller_(controller), itemSearch_(new QLineEdit),
      itemCategory_(new QComboBox), items_(new QListView),
      itemTitle_(new QLabel("Select equipment")), buildSearch_(new QLineEdit),
      scope_(new QComboBox), sort_(new QComboBox), builds_(new QListView),
      empty_(new QLabel), buildTitle_(new QLabel), buildMeta_(new QLabel),
      topology_(new BuildTopologyWidget(controller)), noteTitle_(new QLabel),
      note_(new QLabel), state_(new QLabel),
      add_(new QPushButton("Add to group")), openGroup_(new QPushButton("Open group")), itemSearchTimer_(new QTimer(this)),
      buildSearchTimer_(new QTimer(this)) {
  auto *layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(8);

  auto *itemFilters = new QHBoxLayout;
  itemSearch_->setPlaceholderText("Search equipment");
  itemSearch_->setClearButtonEnabled(true);
  itemFilters->addWidget(itemSearch_, 1);
  const QList<QPair<QString, QString>> categories = {
      {"All equipment", "all"}, {"Warframes", "warframe"},
      {"Primary", "primary"},   {"Secondary", "secondary"},
      {"Melee", "melee"},       {"Companions", "companion"},
      {"Vehicles", "vehicle"},
  };
  for (const auto &[label, value] : categories) {
    itemCategory_->addItem(label, value);
  }
  itemFilters->addWidget(itemCategory_);
  layout->addLayout(itemFilters);

  items_->setObjectName("buildSourceItemList");
  items_->setModel(controller_->buildSourceItems());
  items_->setSelectionMode(QAbstractItemView::SingleSelection);
  items_->setUniformItemSizes(true);
  items_->setMaximumHeight(150);
  layout->addWidget(items_);

  auto *buildFilters = new QHBoxLayout;
  itemTitle_->setObjectName("sectionTitle");
  buildFilters->addWidget(itemTitle_);
  buildFilters->addStretch();
  buildSearch_->setPlaceholderText("Filter builds");
  buildSearch_->setClearButtonEnabled(true);
  buildFilters->addWidget(buildSearch_);
  scope_->addItem("Public", "public");
  scope_->addItem("Favorites", "favorites");
  scope_->addItem("My builds", "mine");
  buildFilters->addWidget(scope_);
  sort_->addItem("Top rated", "score");
  sort_->addItem("Recently updated", "updated");
  sort_->addItem("Least Forma", "formas");
  sort_->addItem("Most Forma", "formas_desc");
  buildFilters->addWidget(sort_);
  layout->addLayout(buildFilters);

  auto *splitter = new QSplitter;
  splitter->setObjectName("buildDiscoverSplit");
  splitter->setChildrenCollapsible(false);
  auto *results = new QFrame;
  results->setObjectName("buildPane");
  results->setMinimumWidth(280);
  auto *resultsLayout = new QVBoxLayout(results);
  resultsLayout->setContentsMargins(8, 8, 8, 8);
  builds_->setObjectName("buildSourceList");
  builds_->setModel(controller_->sourceBuilds());
  builds_->setSelectionMode(QAbstractItemView::SingleSelection);
  builds_->setUniformItemSizes(true);
  builds_->setSpacing(4);
  resultsLayout->addWidget(builds_, 1);
  empty_->setObjectName("emptyState");
  empty_->setAlignment(Qt::AlignCenter);
  empty_->setWordWrap(true);
  resultsLayout->addWidget(empty_);
  splitter->addWidget(results);

  auto *detail = new QFrame;
  detail->setObjectName("buildPane");
  detail->setMinimumWidth(360);
  auto *detailLayout = new QVBoxLayout(detail);
  detailLayout->setContentsMargins(14, 12, 14, 12);
  detailLayout->setSpacing(7);
  buildTitle_->setObjectName("sectionTitle");
  buildMeta_->setObjectName("secondaryText");
  buildMeta_->setWordWrap(true);
  auto *revisionScroll = new QScrollArea;
  revisionScroll->setObjectName("buildRevisionScroll");
  revisionScroll->setWidgetResizable(true);
  revisionScroll->setFrameShape(QFrame::NoFrame);
  auto *revisionBody = new QWidget;
  revisionBody->setObjectName("buildRevisionBody");
  auto *revisionLayout = new QVBoxLayout(revisionBody);
  revisionLayout->setContentsMargins(0, 0, 0, 0);
  revisionLayout->setSpacing(8);
  revisionLayout->addWidget(topology_);
  noteTitle_->setObjectName("sectionTitle");
  noteTitle_->setText("Build notes");
  note_->setObjectName("buildNote");
  note_->setTextFormat(Qt::MarkdownText);
  note_->setWordWrap(true);
  note_->setTextInteractionFlags(Qt::TextBrowserInteraction);
  note_->setOpenExternalLinks(true);
  noteTitle_->hide();
  note_->hide();
  revisionLayout->addWidget(noteTitle_);
  revisionLayout->addWidget(note_);
  revisionLayout->addStretch();
  revisionScroll->setWidget(revisionBody);
  state_->setObjectName("buildWorkspaceState");
  state_->setWordWrap(true);
  add_->setObjectName("primaryAction");
  detailLayout->addWidget(buildTitle_);
  detailLayout->addWidget(buildMeta_);
  detailLayout->addWidget(revisionScroll, 1);
  detailLayout->addWidget(state_);
  auto *actions = new QHBoxLayout;
  openGroup_->setObjectName("textAction");
  openGroup_->setProperty("testId", "buildAddedGroup");
  openGroup_->hide();
  actions->addWidget(openGroup_);
  actions->addStretch();
  actions->addWidget(add_);
  detailLayout->addLayout(actions);
  splitter->addWidget(detail);
  splitter->setStretchFactor(0, 2);
  splitter->setStretchFactor(1, 3);
  splitter->setSizes({400, 600});
  layout->addWidget(splitter, 1);

  itemSearchTimer_->setSingleShot(true);
  itemSearchTimer_->setInterval(250);
  buildSearchTimer_->setSingleShot(true);
  buildSearchTimer_->setInterval(300);
  connect(itemSearchTimer_, &QTimer::timeout, this,
          &BuildDiscoverWidget::requestItemSearch);
  connect(buildSearchTimer_, &QTimer::timeout, this,
          [this] { requestBuilds(); });
  connect(itemSearch_, &QLineEdit::textChanged, itemSearchTimer_,
          qOverload<>(&QTimer::start));
  connect(itemCategory_, &QComboBox::currentIndexChanged, itemSearchTimer_,
          qOverload<>(&QTimer::start));
  connect(buildSearch_, &QLineEdit::textChanged, buildSearchTimer_,
          qOverload<>(&QTimer::start));
  connect(scope_, &QComboBox::currentIndexChanged, this,
          [this] { requestBuilds(); });
  connect(sort_, &QComboBox::currentIndexChanged, this,
          [this] { requestBuilds(); });
  connect(items_->selectionModel(), &QItemSelectionModel::currentChanged, this,
          [this](const QModelIndex &index) { selectItemIndex(index); });
  connect(builds_->selectionModel(), &QItemSelectionModel::currentChanged, this,
          [this](const QModelIndex &index) { selectBuild(index); });
  connect(add_, &QPushButton::clicked, this,
          &BuildDiscoverWidget::showGroupMenu);
  connect(openGroup_, &QPushButton::clicked, this, [this] {
    emit groupRequested(addedGroupId_);
  });
  connect(controller_, &AppController::buildSourceItemsStateChanged, this,
          [this] {
            restoreItemSelection();
            updateState();
          });
  connect(controller_, &AppController::sourceBuildsStateChanged, this, [this] {
    restoreBuildSelection();
    updateState();
  });
  connect(controller_, &AppController::buildRevisionChanged, this,
          [this](qint64 id) {
            if (id == selectedBuildId_) {
              showRevision(id);
              updateState();
            }
          });
  connect(controller_, &AppController::overframeAccountChanged, this,
          &BuildDiscoverWidget::updateState);
  connect(controller_, &AppController::buildGroupsStateChanged, this,
          &BuildDiscoverWidget::updateState);
  connect(controller_, &AppController::buildGroupRequestFinished, this,
          [this](const QJsonObject &request, const QJsonObject &group) {
            const QString op = request.value("op").toString();
            if (op == "build_group_create" && pendingCreate_) {
              pendingCreate_ = false;
              pendingGroupId_ = group.value("id").toString();
              addToGroup(group);
            } else if (op == "build_group_add_source" &&
                       group.value("id").toString() == pendingGroupId_) {
              addedGroupId_ = pendingGroupId_;
              pendingGroupId_.clear();
              state_->setText("Added to " + group.value("name").toString());
              openGroup_->show();
            }
            updateState();
          });
  connect(controller_, &AppController::buildGroupRequestFailed, this,
          [this](const QJsonObject &, const QString &) {
            pendingCreate_ = false;
            pendingGroupId_.clear();
            updateState();
          });
  updateState();
}

void BuildDiscoverWidget::selectItem(const QString &definitionId) {
  selectedItemId_ = definitionId;
  restoreItemSelection();
  if (items_->currentIndex().data(BuildItemModel::CanonicalIdRole).toString() !=
      definitionId) {
    controller_->searchBuildItems(definitionId, "all");
  }
}

void BuildDiscoverWidget::ensureLoaded() {
  if (controller_->buildSourceItems()->rowCount() == 0) {
    requestItemSearch();
  }
}

void BuildDiscoverWidget::requestItemSearch() {
  controller_->searchBuildItems(itemSearch_->text(),
                                itemCategory_->currentData().toString());
  updateState();
}

void BuildDiscoverWidget::requestBuilds(bool refresh) {
  if (selectedItemId_.isEmpty()) {
    return;
  }
  buildListItemId_ = selectedItemId_;
  controller_->requestSourceBuilds(
      selectedItemId_, buildSearch_->text(), scope_->currentData().toString(),
      sort_->currentData().toString(), 50, 0, refresh);
  updateState();
}

void BuildDiscoverWidget::restoreItemSelection() {
  QAbstractItemModel *model = controller_->buildSourceItems();
  QModelIndex selected;
  for (int row = 0; row < model->rowCount(); ++row) {
    const QModelIndex candidate = model->index(row, 0);
    if (candidate.data(BuildItemModel::CanonicalIdRole).toString() ==
        selectedItemId_) {
      selected = candidate;
      break;
    }
  }
  if (!selected.isValid() && selectedItemId_.isEmpty() &&
      model->rowCount() > 0) {
    selected = model->index(0, 0);
  }
  if (selected.isValid()) {
    items_->setCurrentIndex(selected);
    selectItemIndex(selected);
  }
}

void BuildDiscoverWidget::restoreBuildSelection() {
  QAbstractItemModel *model = controller_->sourceBuilds();
  QModelIndex selected;
  for (int row = 0; row < model->rowCount(); ++row) {
    const QModelIndex candidate = model->index(row, 0);
    if (candidate.data(BuildSummaryModel::ExternalIdRole).toLongLong() ==
        selectedBuildId_) {
      selected = candidate;
      break;
    }
  }
  if (!selected.isValid() && model->rowCount() > 0) {
    selected = model->index(0, 0);
  }
  if (selected.isValid()) {
    builds_->setCurrentIndex(selected);
    selectBuild(selected);
  } else {
    selectedBuildId_ = 0;
    buildTitle_->clear();
    buildMeta_->clear();
    topology_->clear();
    note_->clear();
    note_->hide();
    noteTitle_->hide();
  }
}

void BuildDiscoverWidget::selectItemIndex(const QModelIndex &index) {
  if (!index.isValid()) {
    return;
  }
  const QString id = index.data(BuildItemModel::CanonicalIdRole).toString();
  const bool changed = selectedItemId_ != id;
  selectedItemId_ = id;
  selectedItemName_ = index.data(BuildItemModel::NameRole).toString();
  itemTitle_->setText(selectedItemName_);
  if (changed) {
    selectedBuildId_ = 0;
  }
  if (changed || buildListItemId_ != id) {
    requestBuilds();
  }
}

void BuildDiscoverWidget::selectBuild(const QModelIndex &index) {
  if (!index.isValid()) {
    return;
  }
  selectedBuildId_ = index.data(BuildSummaryModel::ExternalIdRole).toLongLong();
  openGroup_->hide();
  buildTitle_->setText(index.data(BuildSummaryModel::TitleRole).toString());
  QStringList meta;
  const QString author = index.data(BuildSummaryModel::AuthorRole).toString();
  if (!author.isEmpty()) {
    meta.append(author);
  }
  meta.append(QString("score %1")
                  .arg(index.data(BuildSummaryModel::ScoreRole).toInt()));
  meta.append(QString("%1 Forma")
                  .arg(index.data(BuildSummaryModel::FormasRole).toInt()));
  const QString updated =
      buildDate(index.data(BuildSummaryModel::UpdatedAtRole).toString());
  if (!updated.isEmpty()) {
    meta.append("updated " + updated);
  }
  buildMeta_->setText(meta.join("  ·  "));
  state_->setText("Loading build details...");
  topology_->clear();
  note_->clear();
  note_->hide();
  noteTitle_->hide();
  controller_->requestBuildRevision(selectedBuildId_);
  showRevision(selectedBuildId_);
}

void BuildDiscoverWidget::showRevision(qint64 id) {
  const QString error = controller_->buildRevisionError(id);
  if (!error.isEmpty()) {
    state_->setText(error);
    topology_->clear();
    note_->clear();
    note_->hide();
    noteTitle_->hide();
    return;
  }
  const QJsonObject revision = controller_->buildRevision(id);
  if (revision.isEmpty()) {
    return;
  }
  const QJsonObject metadata = revision.value("metadata").toObject();
  buildTitle_->setText(metadata.value("title").toString(buildTitle_->text()));
  topology_->setSourceRevision(revision, {});
  const QString description =
      metadata.value("description").toString().trimmed();
  note_->setText(description);
  note_->setVisible(!description.isEmpty());
  noteTitle_->setVisible(!description.isEmpty());
  state_->setText("Revision " +
                  revision.value("fingerprint").toString().left(12));
}

void BuildDiscoverWidget::updateState() {
  const bool authenticated =
      controller_->overframeAccount().value("authenticated").toBool();
  if (auto *model = qobject_cast<QStandardItemModel *>(scope_->model())) {
    for (int row = 1; row < model->rowCount(); ++row) {
      model->item(row)->setEnabled(authenticated);
    }
  }
  if (!authenticated && scope_->currentData().toString() != "public") {
    scope_->setCurrentIndex(0);
  }
  const bool hasItem = !selectedItemId_.isEmpty();
  buildSearch_->setEnabled(hasItem);
  scope_->setEnabled(hasItem);
  sort_->setEnabled(hasItem);
  const bool noBuilds = hasItem && !controller_->sourceBuildsLoading() &&
                        controller_->sourceBuilds()->rowCount() == 0;
  builds_->setVisible(hasItem && !noBuilds);
  empty_->setVisible(noBuilds);
  empty_->setText(noBuilds ? "No builds found." : QString());
  const bool hasRevision =
      selectedBuildId_ > 0 &&
      !controller_->buildRevision(selectedBuildId_).isEmpty();
  add_->setEnabled(hasRevision && !controller_->buildGroupsLoading());
}

void BuildDiscoverWidget::showGroupMenu() {
  if (selectedItemId_.isEmpty() || selectedBuildId_ <= 0) {
    return;
  }
  controller_->ensureBuildGroups();
  QMenu menu(this);
  QAbstractItemModel *groups = controller_->buildGroups();
  for (int row = 0; row < groups->rowCount(); ++row) {
    const QModelIndex index = groups->index(row, 0);
    if (index.data(BuildGroupModel::DefinitionIdRole).toString() !=
        selectedItemId_) {
      continue;
    }
    QAction *action = menu.addAction(index.data().toString());
    action->setData(index.data(BuildGroupModel::RawRole));
  }
  if (!menu.actions().isEmpty()) {
    menu.addSeparator();
  }
  QAction *create = menu.addAction("New group...");
  QAction *selected = menu.exec(add_->mapToGlobal(QPoint(0, add_->height())));
  if (!selected) {
    return;
  }
  if (selected == create) {
    createGroupAndAdd();
  } else {
    addToGroup(QJsonObject::fromVariantMap(selected->data().toMap()));
  }
}

void BuildDiscoverWidget::createGroupAndAdd() {
  bool accepted = false;
  const QString name =
      QInputDialog::getText(this, "New build group", "Name", QLineEdit::Normal,
                            selectedItemName_ + " builds", &accepted);
  if (!accepted || name.trimmed().isEmpty()) {
    return;
  }
  pendingCreate_ = true;
  controller_->createBuildGroup(
      {{"name", name.trimmed()}, {"definition_id", selectedItemId_}});
}

void BuildDiscoverWidget::addToGroup(const QJsonObject &group) {
  const QJsonObject revision = controller_->buildRevision(selectedBuildId_);
  const QString id = group.value("id").toString();
  if (id.isEmpty() || revision.isEmpty()) {
    pendingGroupId_.clear();
    return;
  }
  pendingGroupId_ = id;
  controller_->addBuildSourceToGroup(id, group.value("revision").toInteger(),
                                     selectedBuildId_,
                                     revision.value("fingerprint").toString());
}
