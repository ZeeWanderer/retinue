#pragma once

#include <QString>
#include <QWidget>

class QLineEdit;
class QVariantAnimation;

class CompactSearch final : public QWidget {
public:
  explicit CompactSearch(const QString &placeholder, QWidget *parent = nullptr);

  QLineEdit *editor() const;
  void setText(const QString &text);
  QSize sizeHint() const override;
  QSize minimumSizeHint() const override;

private:
  void expand();
  void collapse();
  void animateTo(int width);
  int expandedWidth() const;

  QLineEdit *editor_;
  QVariantAnimation *animation_;
  int preferredWidth_ = 40;
  bool expanded_ = false;
};
