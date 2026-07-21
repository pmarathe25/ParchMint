#include <QCommandLineParser>
#include <QDateTime>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QIcon>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLibraryInfo>
#include <QQmlApplicationEngine>
#include <QMetaObject>
#include <QQuickStyle>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTimer>

#include <cstdio>
#include <exception>
#include <memory>

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
  QCommandLineOption lifecycleSmoke(
    QStringLiteral("lifecycle-smoke-test"),
    QStringLiteral("Exercise live typing, pane transitions, export, trash, and project close offscreen."));
  parser.addOption(smoke);
  parser.addOption(lifecycleSmoke);
  parser.process(application);

  QQmlApplicationEngine engine;
  QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &application,
                   [] { QCoreApplication::exit(2); }, Qt::QueuedConnection);
  engine.loadFromModule(QStringLiteral("org.parchmint.app"), QStringLiteral("Main"));

  QObject* backend = nullptr;
  if (!engine.rootObjects().isEmpty()) {
    backend = engine.rootObjects().constFirst()->findChild<QObject*>(
      QStringLiteral("parchmintBackend"));
    if (backend) {
      QObject::connect(&application, &QCoreApplication::aboutToQuit, backend, [backend] {
        // Normal window close already performed this bounded flush. Repeating
        // it here covers platform/session-manager quit paths that bypass the
        // QML close handler; a failed canonical attempt gets one last
        // journal-only fallback and never starts unbounded shutdown I/O.
        bool prepared = false;
        QMetaObject::invokeMethod(backend, "prepareQuit", Qt::DirectConnection,
                                  Q_RETURN_ARG(bool, prepared));
        if (!prepared)
          QMetaObject::invokeMethod(backend, "emergencyJournal", Qt::DirectConnection);
      });
    }
  }

  std::unique_ptr<QTemporaryDir> lifecycleDirectory;
  if (parser.isSet(lifecycleSmoke)) {
    if (!backend) {
      std::fprintf(stderr, "lifecycle smoke: backend object was not created\n");
      return 3;
    }
    lifecycleDirectory = std::make_unique<QTemporaryDir>();
    if (!lifecycleDirectory->isValid()) {
      std::fprintf(stderr, "lifecycle smoke: temporary project directory failed\n");
      return 3;
    }
    QTimer::singleShot(0, &application,
                       [&application, &engine, &lifecycleDirectory, backend] {
      const auto fail = [&application](const char* message) {
        std::fprintf(stderr, "lifecycle smoke: %s\n", message);
        application.exit(3);
      };
      bool succeeded = false;
      const QString parent = lifecycleDirectory->path();
      const QString projectName = QStringLiteral("Live Safety");
      if (!QMetaObject::invokeMethod(
            backend, "createProject", Qt::DirectConnection, Q_RETURN_ARG(bool, succeeded),
            Q_ARG(QString, parent), Q_ARG(QString, projectName))
          || !succeeded) {
        fail("project creation failed");
        return;
      }

      QString manuscriptRoot;
      if (!QMetaObject::invokeMethod(
            backend, "nodeId", Qt::DirectConnection, Q_RETURN_ARG(QString, manuscriptRoot),
            Q_ARG(int, 0))
          || manuscriptRoot.isEmpty()) {
        fail("manuscript root was unavailable");
        return;
      }
      succeeded = false;
      if (!QMetaObject::invokeMethod(
            backend, "createChild", Qt::DirectConnection, Q_RETURN_ARG(bool, succeeded),
            Q_ARG(QString, manuscriptRoot), Q_ARG(QString, QStringLiteral("Live Scene")),
            Q_ARG(bool, false))
          || !succeeded) {
        fail("document creation failed");
        return;
      }
      const QString documentId = backend->property("selected_id").toString();
      if (documentId.isEmpty()
          || !QMetaObject::invokeMethod(
            backend, "selectNode", Qt::DirectConnection, Q_ARG(QString, documentId),
            Q_ARG(bool, false))) {
        fail("document navigation failed");
        return;
      }
      const QString projectRoot = QDir(parent).filePath(projectName);
      const QString canonicalPath = QDir(projectRoot).filePath(
        QStringLiteral("manuscript/%1.md").arg(documentId));
      QFile originalCanonical(canonicalPath);
      if (!originalCanonical.open(QIODevice::ReadOnly)) {
        fail("canonical document was unavailable before typing");
        return;
      }
      const QByteArray originalCanonicalBytes = originalCanonical.readAll();
      application.processEvents();

      auto* editor = engine.rootObjects().constFirst()->findChild<QObject*>(
        QStringLiteral("paneEditor0"));
      const QString liveText = QStringLiteral("Typed without focus loss — 本#%.\n");
      const auto ffiBytesBeforeTyping = backend->property("ffi_bytes").toULongLong();
      if (!editor || !editor->setProperty("text", liveText)) {
        fail("QML editor text injection failed");
        return;
      }
      application.processEvents();
      QString authoritativeBody;
      if (!QMetaObject::invokeMethod(
            backend, "paneDocumentBody", Qt::DirectConnection,
            Q_RETURN_ARG(QString, authoritativeBody), Q_ARG(int, 0))
          || authoritativeBody != liveText) {
        fail("QML text did not reach the authoritative live session");
        return;
      }
      const auto ffiTypingBytes = backend->property("ffi_bytes").toULongLong()
                                  - ffiBytesBeforeTyping;
      if (ffiTypingBytes > 4'096) {
        fail("one editor change crossed an unbounded FFI payload");
        return;
      }
      std::fprintf(stdout, "lifecycle ffi_typing_bytes=%llu\n", ffiTypingBytes);
      succeeded = false;
      if (!QMetaObject::invokeMethod(
            backend, "emergencyJournal", Qt::DirectConnection, Q_RETURN_ARG(bool, succeeded))
          || !succeeded) {
        fail("recovery journal could not capture unfocused typing");
        return;
      }
      QDir recoveryDirectory(QDir(projectRoot).filePath(QStringLiteral(".parchmint/recovery")));
      const QStringList recoveryFiles = recoveryDirectory.entryList(
        { QStringLiteral("*.toml") }, QDir::Files, QDir::Name);
      if (recoveryFiles.size() != 1) {
        fail("recovery journal was not created");
        return;
      }
      QFile recoveryFile(recoveryDirectory.filePath(recoveryFiles.constFirst()));
      if (!recoveryFile.open(QIODevice::ReadOnly)) {
        fail("recovery journal could not be inspected");
        return;
      }
      const QByteArray recoveryBytes = recoveryFile.readAll();
      const QString recoveryFileName = recoveryFiles.constFirst();

      succeeded = false;
      if (!QMetaObject::invokeMethod(
            backend, "swapPanes", Qt::DirectConnection, Q_RETURN_ARG(bool, succeeded))
          || !succeeded) {
        fail("pane swap vetoed current text");
        return;
      }
      succeeded = false;
      if (!QMetaObject::invokeMethod(
            backend, "closePane", Qt::DirectConnection, Q_RETURN_ARG(bool, succeeded),
            Q_ARG(int, 1))
          || !succeeded) {
        fail("pane close vetoed current text");
        return;
      }
      if (!QMetaObject::invokeMethod(
            backend, "selectNode", Qt::DirectConnection, Q_ARG(QString, documentId),
            Q_ARG(bool, false))) {
        fail("document reopen failed");
        return;
      }

      const QString exportPath = lifecycleDirectory->filePath(QStringLiteral("live-export.md"));
      succeeded = false;
      if (!QMetaObject::invokeMethod(
            backend, "exportProject", Qt::DirectConnection, Q_RETURN_ARG(bool, succeeded),
            Q_ARG(QString, QStringLiteral("markdown")), Q_ARG(QString, exportPath))
          || !succeeded) {
        fail("export did not start");
        return;
      }

      auto* poll = new QTimer(&application);
      auto* elapsed = new QElapsedTimer();
      elapsed->start();
      QObject::connect(poll, &QTimer::timeout, &application,
                       [&application, &engine, backend, canonicalPath, documentId, exportPath,
                        liveText, originalCanonicalBytes, poll, elapsed, projectRoot,
                        recoveryBytes, recoveryFileName, fail] {
        QMetaObject::invokeMethod(backend, "pollExport", Qt::DirectConnection);
        if (backend->property("export_in_progress").toBool()) {
          if (elapsed->elapsed() > 10000) {
            poll->stop();
            delete elapsed;
            fail("export worker timed out");
          }
          return;
        }
        poll->stop();
        delete elapsed;
        QFile exported(exportPath);
        if (!exported.open(QIODevice::ReadOnly) || !exported.readAll().contains(liveText.toUtf8())) {
          fail("export omitted the latest live revision");
          return;
        }
        bool succeeded = false;
        if (!QMetaObject::invokeMethod(
              backend, "closeProject", Qt::DirectConnection, Q_RETURN_ARG(bool, succeeded))
            || !succeeded) {
          fail("project close failed");
          return;
        }

        QFile canonical(canonicalPath);
        if (!canonical.open(QIODevice::WriteOnly | QIODevice::Truncate)
            || canonical.write(originalCanonicalBytes) != originalCanonicalBytes.size()) {
          fail("crash-state canonical simulation failed");
          return;
        }
        canonical.close();
        const QString recoveryPath = QDir(projectRoot).filePath(
          QStringLiteral(".parchmint/recovery/%1").arg(recoveryFileName));
        QDir().mkpath(QFileInfo(recoveryPath).absolutePath());
        QFile recovery(recoveryPath);
        if (!recovery.open(QIODevice::WriteOnly | QIODevice::Truncate)
            || recovery.write(recoveryBytes) != recoveryBytes.size()) {
          fail("crash-state recovery simulation failed");
          return;
        }
        recovery.close();

        succeeded = false;
        if (!QMetaObject::invokeMethod(
              backend, "openProject", Qt::DirectConnection, Q_RETURN_ARG(bool, succeeded),
              Q_ARG(QString, projectRoot))
            || !succeeded) {
          fail("project reopen with recovery failed");
          return;
        }
        application.processEvents();
        auto* recoveryDialog = engine.rootObjects().constFirst()->findChild<QObject*>(
          QStringLiteral("recoveryDialog"));
        if (backend->property("recovery_count").toInt() != 1
            || !recoveryDialog || !recoveryDialog->property("visible").toBool()) {
          fail("recovery choice was not reachable in the shipping QML");
          return;
        }
        succeeded = false;
        if (!QMetaObject::invokeMethod(
              backend, "restoreRecovery", Qt::DirectConnection, Q_RETURN_ARG(bool, succeeded))
            || !succeeded) {
          fail("recovery restore failed");
          return;
        }
        succeeded = false;
        if (!QMetaObject::invokeMethod(
              backend, "flushAllDocuments", Qt::DirectConnection, Q_RETURN_ARG(bool, succeeded))
            || !succeeded) {
          fail("restored revision did not flush");
          return;
        }
        application.processEvents();
        if (backend->property("recovery_count").toInt() != 0) {
          fail("recovery choice did not resolve cleanly");
          return;
        }
        succeeded = false;
        if (!QMetaObject::invokeMethod(
              backend, "trashNode", Qt::DirectConnection, Q_RETURN_ARG(bool, succeeded),
              Q_ARG(QString, documentId))
            || !succeeded) {
          fail("trash transition failed");
          return;
        }
        succeeded = false;
        if (!QMetaObject::invokeMethod(
              backend, "closeProject", Qt::DirectConnection, Q_RETURN_ARG(bool, succeeded))
            || !succeeded) {
          fail("final project close failed");
          return;
        }
        application.exit(0);
      });
      poll->start(10);
    });
  }

  if (parser.isSet(smoke))
    QTimer::singleShot(300, &application, &QCoreApplication::quit);
  return application.exec();
}
