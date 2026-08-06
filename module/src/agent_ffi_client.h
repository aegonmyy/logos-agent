#ifndef AGENT_FFI_CLIENT_H
#define AGENT_FFI_CLIENT_H

#include <QLibrary>
#include <QString>

// Lazy-loads liblogos_agent.so (the Rust agent core) and exposes its C
// entry-points. The library path can be overridden with LOGOS_AGENT_FFI_PATH;
// otherwise the loader searches the default library name on the system path.
class AgentFfiClient
{
public:
    QString version();
    QString skills();

    // Live agent session: start it for an account, invoke skills by name with
    // JSON args, then stop it.
    bool startSession(const QString& accountId);
    QString invoke(const QString& name, const QString& argsJson);
    void stopSession();

    QString lastError() const { return m_lastErr; }
    ~AgentFfiClient();

private:
    using NoArgFn        = char* (*)();
    using FreeFn         = void  (*)(char*);
    using SessionNewFn   = void* (*)(const char*);
    using SessionInvokeFn = char* (*)(void*, const char*, const char*);
    using SessionFreeFn  = void  (*)(void*);

    bool load(QString* err = nullptr);
    QString callNoArg(NoArgFn fn, const char* name);

    QLibrary        m_lib;
    NoArgFn         m_version       = nullptr;
    NoArgFn         m_skills        = nullptr;
    FreeFn          m_free          = nullptr;
    SessionNewFn    m_sessionNew    = nullptr;
    SessionInvokeFn m_sessionInvoke = nullptr;
    SessionFreeFn   m_sessionFree   = nullptr;
    void*           m_session       = nullptr;
    bool            m_loaded        = false;
    QString         m_lastErr;
};

#endif // AGENT_FFI_CLIENT_H
