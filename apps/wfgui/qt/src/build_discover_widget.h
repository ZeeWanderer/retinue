#pragma once

#include <QWidget>

class AppController;
class QLabel;
class QComboBox;
class QLineEdit;
class QListView;
class QModelIndex;
class QPushButton;
class QTimer;
class BuildTopologyWidget;

class BuildDiscoverWidget final : public QWidget {
  Q_OBJECT

public:
  explicit BuildDiscoverWidget(AppController *controller,
                               QWidget *parent = nullptr);
  void selectItem(const QString &definitionId);
  void ensureLoaded();

signals:
  void groupRequested(const QString &groupId);

private:
  void requestItemSearch();
  void requestBuilds(bool refresh = false);
  void restoreItemSelection();
  void restoreBuildSelection();
  void selectItemIndex(const QModelIndex &index);
  void selectBuild(const QModelIndex &index);
  void showRevision(qint64 id);
  void updateState();
  void showGroupMenu();
  void createGroupAndAdd();
  void addToGroup(const QJsonObject &group);

  AppController *controller_;
  QLineEdit *itemSearch_;
  QComboBox *itemCategory_;
  QListView *items_;
  QLabel *itemTitle_;
  QLineEdit *buildSearch_;
  QComboBox *scope_;
  QComboBox *sort_;
  QListView *builds_;
  QLabel *empty_;
  QLabel *buildTitle_;
  QLabel *buildMeta_;
  BuildTopologyWidget *topology_;
  QLabel *noteTitle_;
  QLabel *note_;
  QLabel *state_;
  QPushButton *add_;
  QPushButton *openGroup_;
  QTimer *itemSearchTimer_;
  QTimer *buildSearchTimer_;
  QString selectedItemId_;
  QString selectedItemName_;
  QString buildListItemId_;
  qint64 selectedBuildId_ = 0;
  QString pendingGroupId_;
  QString addedGroupId_;
  bool pendingCreate_ = false;
};
