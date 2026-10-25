#pragma once

#include <QSpinBox>

namespace wfgui {

class MarketSpinBox final : public QSpinBox {
public:
  explicit MarketSpinBox(QWidget *parent = nullptr);

protected:
  void paintEvent(QPaintEvent *event) override;
};

} // namespace wfgui
