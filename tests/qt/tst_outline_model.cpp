#include "models/outline_model.h"

#include <QTest>

class FakeRustOutline final : public QObject
{
  Q_OBJECT
  Q_PROPERTY(int node_count READ nodeCount CONSTANT)

public:
  int nodeCount() const { return 10'000; }
  Q_INVOKABLE QString nodeTitle(qint32 row) const { return QStringLiteral("Node %1").arg(row); }
  Q_INVOKABLE qint32 nodeDepth(qint32 row) const { return row == 0 ? 0 : 2; }
  Q_INVOKABLE qint32 nodeParent(qint32 row) const { return row == 0 ? -1 : 0; }
};

class OutlineModelTest final : public QObject
{
  Q_OBJECT

private slots:
  void lazilyQueriesRustShapedSource()
  {
    FakeRustOutline source;
    OutlineModel model;
    model.setSource(&source);
    QCOMPARE(model.rowCount(), 10'000);
    QCOMPARE(model.data(model.index(9'999), OutlineModel::TitleRole).toString(),
             QStringLiteral("Node 9999"));
    QCOMPARE(model.data(model.index(9'999), OutlineModel::DepthRole).toInt(), 2);
    QCOMPARE(model.data(model.index(9'999), OutlineModel::ParentIdRole).toInt(), 0);
  }
};

QTEST_MAIN(OutlineModelTest)
#include "tst_outline_model.moc"
