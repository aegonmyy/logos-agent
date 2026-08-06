#include "agent_ffi_client.h"

#include <QByteArray>

namespace {
const char* kLib    = "liblogos_agent.so";
const char* kEnvKey = "LOGOS_AGENT_FFI_PATH";

QString errJson(const QString& message)
{
    QString escaped = message;
    escaped.replace(QLatin1Char('"'), QLatin1Char('\''));
    return QStringLiteral("{\"ok\":false,\"error\":\"%1\"}").arg(escaped);
}
} // namespace

bool AgentFfiClient::load(QString* err)
{
    if (m_loaded) {
        return true;
    }

    QString libPath = qEnvironmentVariable(kEnvKey);
    if (libPath.isEmpty()) {
        libPath = QLatin1String(kLib);
    }

    m_lib.setFileName(libPath);
    if (!m_lib.load()) {
        m_lastErr = QStringLiteral("cannot load %1: %2").arg(libPath, m_lib.errorString());
        if (err) {
            *err = m_lastErr;
        }
        return false;
    }

    m_version = reinterpret_cast<NoArgFn>(m_lib.resolve("logos_agent_version"));
    m_skills  = reinterpret_cast<NoArgFn>(m_lib.resolve("logos_agent_default_skills"));
    m_free    = reinterpret_cast<FreeFn>(m_lib.resolve("logos_agent_free_string"));
    m_sessionNew    = reinterpret_cast<SessionNewFn>(m_lib.resolve("logos_agent_session_new_offline"));
    m_sessionInvoke = reinterpret_cast<SessionInvokeFn>(m_lib.resolve("logos_agent_session_invoke"));
    m_sessionFree   = reinterpret_cast<SessionFreeFn>(m_lib.resolve("logos_agent_session_free"));

    if (!m_version || !m_skills || !m_free
        || !m_sessionNew || !m_sessionInvoke || !m_sessionFree) {
        m_lastErr = QStringLiteral("missing symbols in %1").arg(m_lib.fileName());
        if (err) {
            *err = m_lastErr;
        }
        m_lib.unload();
        return false;
    }

    m_loaded = true;
    return true;
}

QString AgentFfiClient::callNoArg(NoArgFn fn, const char* name)
{
    char* result = fn();
    if (!result) {
        return errJson(QStringLiteral("%1 returned null").arg(QString::fromLatin1(name)));
    }
    const QString text = QString::fromUtf8(result);
    m_free(result);
    return text;
}

QString AgentFfiClient::version()
{
    if (!load()) {
        return errJson(m_lastErr);
    }
    return callNoArg(m_version, "version");
}

QString AgentFfiClient::skills()
{
    if (!load()) {
        return errJson(m_lastErr);
    }
    return callNoArg(m_skills, "skills");
}

bool AgentFfiClient::startSession(const QString& accountId)
{
    if (!load()) {
        return false;
    }
    if (m_session) {
        return true;
    }
    const QByteArray id = accountId.toUtf8();
    m_session = m_sessionNew(id.constData());
    if (!m_session) {
        m_lastErr = QStringLiteral("could not start agent session");
    }
    return m_session != nullptr;
}

QString AgentFfiClient::invoke(const QString& name, const QString& argsJson)
{
    if (!load()) {
        return errJson(m_lastErr);
    }
    if (!m_session) {
        return errJson(QStringLiteral("no active agent session"));
    }
    const QByteArray n = name.toUtf8();
    const QByteArray a = argsJson.toUtf8();
    char* result = m_sessionInvoke(m_session, n.constData(), a.constData());
    if (!result) {
        return errJson(QStringLiteral("invoke returned null"));
    }
    const QString text = QString::fromUtf8(result);
    m_free(result);
    return text;
}

void AgentFfiClient::stopSession()
{
    if (m_session && m_sessionFree) {
        m_sessionFree(m_session);
        m_session = nullptr;
    }
}

AgentFfiClient::~AgentFfiClient()
{
    stopSession();
}
