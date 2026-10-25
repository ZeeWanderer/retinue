#pragma once

#include <QWidget>

class QLabel;
class QMouseEvent;
class QPaintEvent;
class QToolButton;

class TitleBarWidget final : public QWidget {
  Q_OBJECT

public:
  explicit TitleBarWidget(QWidget *parent = nullptr);
  void setMaximized(bool maximized);
  void setLeftRailCollapsed(bool collapsed);
  void setRightRailCollapsed(bool collapsed);
  void setRightRailAvailable(bool available);

signals:
  void uiScaleDeltaRequested(int delta);
  void leftRailToggleRequested();
  void rightRailToggleRequested();

protected:
  void paintEvent(QPaintEvent *event) override;
  void mousePressEvent(QMouseEvent *event) override;
  void mouseDoubleClickEvent(QMouseEvent *event) override;

private:
  void toggleMaximized();

  QToolButton *leftRail_;
  QToolButton *rightRail_;
  QToolButton *maximize_;
};
