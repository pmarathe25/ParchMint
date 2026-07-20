#include <QCommandLineParser>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QGuiApplication>
#include <QIcon>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLibraryInfo>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QStandardPaths>
#include <QTimer>

#include <exception>

namespace {
void writeDiagnostic(QtMsgType type, const QMessageLogContext& context, const QString& message)
{
  const auto directory = QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation);
  QDir().mkpath(directory);
  QFile file(directory + QStringLiteral("/parchmint.log"));
  if (!file.open(QIODevice::WriteOnly | QIODevice::Append | QIODevice::Text))
    return;
  QJsonObject entry {
    { QStringLiteral("timestamp"), QDateTime::currentDateTimeUtc().toString(Qt::ISODateWithMs) },
    { QStringLiteral("severity"), static_cast<int>(type) },
    { QStringLiteral("message"), message },
    { QStringLiteral("file"), QString::fromUtf8(context.file ? context.file : "") },
    { QStringLiteral("line"), context.line },
  };
  file.write(QJsonDocument(entry).toJson(QJsonDocument::Compact));
  file.write("\n");
  file.flush();
}
}

int main(int argc, char* argv[])
{
  QGuiApplication::setOrganizationName(QStringLiteral("ParchMint"));
  QGuiApplication::setApplicationName(QStringLiteral("ParchMint"));
  QGuiApplication::setApplicationVersion(QStringLiteral(PARCHMINT_VERSION));
  QQuickStyle::setStyle(QStringLiteral("Material"));
  qInstallMessageHandler(writeDiagnostic);
  std::set_terminate([] {
    qFatal("Unhandled native exception; no diagnostic data was transmitted");
  });

  QGuiApplication application(argc, argv);
  QGuiApplication::setWindowIcon(
    QIcon(QStringLiteral(":/org.parchmint.ParchMint.svg")));
  QCommandLineParser parser;
  parser.addHelpOption();
  parser.addVersionOption();
  QCommandLineOption smoke(QStringLiteral("smoke-test"), QStringLiteral("Load the UI offscreen and exit."));
  parser.addOption(smoke);
  parser.process(application);

  QQmlApplicationEngine engine;
  QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &application,
                   [] { QCoreApplication::exit(2); }, Qt::QueuedConnection);
  engine.loadFromModule(QStringLiteral("org.parchmint.app"), QStringLiteral("Main"));

  if (parser.isSet(smoke))
    QTimer::singleShot(300, &application, &QCoreApplication::quit);
  return application.exec();
}
