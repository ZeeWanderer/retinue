#pragma once

#include <QListView>
#include <QString>

class QAbstractItemModel;
class QContextMenuEvent;
class QMouseEvent;
class QResizeEvent;

class RelicGridWidget final : public QListView {
  Q_OBJECT

public:
  explicit RelicGridWidget(QWidget *parent = nullptr);

  void setModel(QAbstractItemModel *model) override;

signals:
  void marketItemRequested(const QString &item, const QString &side);
  void rewardFilterRequested(const QString &reward);
  void foundryItemRequested(const QString &item);

protected:
  bool viewportEvent(QEvent *event) override;
  void contextMenuEvent(QContextMenuEvent *event) override;
  void mouseReleaseEvent(QMouseEvent *event) override;
  void resizeEvent(QResizeEvent *event) override;

private:
  void updateGrid();

  qreal scale_ = 0.0;
};
