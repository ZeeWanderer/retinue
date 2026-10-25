#pragma once

#include <QElapsedTimer>
#include <QProgressBar>

class QTimer;

class AnimatedProgressBar final : public QProgressBar {
  Q_OBJECT

public:
  explicit AnimatedProgressBar(QWidget *parent = nullptr);

  void setRange(int minimum, int maximum);

protected:
  void hideEvent(QHideEvent *event) override;
  void paintEvent(QPaintEvent *event) override;
  void showEvent(QShowEvent *event) override;

private:
  void updateAnimation();

  QTimer *timer_;
  QElapsedTimer elapsed_;
};
