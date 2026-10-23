from __future__ import annotations
import collections.abc
import datetime
import typing
from warnings import deprecated # type: ignore

import jpype # type: ignore
import jpype.protocol # type: ignore

import ghidra
import ghidra.framework
import ghidra.security
import java.io # type: ignore
import java.lang # type: ignore
import java.net.http # type: ignore
import java.security # type: ignore
import java.security.cert # type: ignore
import java.util # type: ignore
import java.util.function # type: ignore
import javax.net.ssl # type: ignore
import javax.security.auth.x500 # type: ignore
import javax.swing.filechooser # type: ignore
import utility.function


class ApplicationKeyManagerFactory(java.lang.Object):
    """
    :obj:`ApplicationKeyManagerFactory` provides a factory for using and caching X509 keystores.
    """

    class_: typing.ClassVar[java.lang.Class]

    @staticmethod
    def clearKeyManagerCache() -> None:
        """
        Clear all cached key managers.
        NOTE: This is primarily intended for test use only.
        """

    @staticmethod
    def getKeyManager(keystorePath: typing.Union[java.lang.String, str], defaultPasswd: typing.Union[java.lang.String, str]) -> javax.net.ssl.X509KeyManager:
        """
        Get key manager for specified JKS or PKCS12 keystore file path.  The user may be prompted
        for a password if required which will block the invocation of this synchronized method.  
        If successfully opened, the resulting key manager instance will be cached for subsequent 
        re-use of the same keystore.
        
        :param java.lang.String or str keystorePath: protected keystore path
        :param java.lang.String or str defaultPasswd: default password (e.g., supplied by property) or null
        :return: key manager
        :rtype: javax.net.ssl.X509KeyManager
        :raises CancelledException: password entry was cancelled by user
        :raises KeyStoreException: error occurred opening/processing keystore
        """

    @staticmethod
    def setKeyStorePasswordProvider(provider: ghidra.security.KeyStorePasswordProvider) -> None:
        """
        Set the active keystore password provider
        
        :param ghidra.security.KeyStorePasswordProvider provider: keystore password provider
        """


class CertTool(ghidra.GhidraLaunchable):

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self) -> None:
        ...


class DefaultKeyManagerFactory(java.lang.Object):
    """
    :obj:`DefaultKeyManagerFactory` provides access to the default application key manager
    associated with the preferred keystore file specified by the :obj:`.KEYSTORE_PATH_PROPERTY`
    system property or set with :meth:`setDefaultKeyStore(String, boolean) <.setDefaultKeyStore>`.
    
    
    Keystore selection depends on the role established via :meth:`initialize(boolean) <.initialize>`
    (client mode is the default for all lazy initialization paths):
    
    * Explicit keystore (client or server) - a keystore file specified via the
    :obj:`.KEYSTORE_PATH_PROPERTY` system property, user preference, or
    :meth:`setDefaultKeyStore(String, boolean) <.setDefaultKeyStore>` is always used when set.  The
    :obj:`.KEYSTORE_PASSWORD_PROPERTY` applies to such file-based keystores only.  This is 
    treated as a default password to access the default keystore file if specified.  In the
    absence of a configured password provider (e.g., server) it must be specified.
    
    * Server - without an explicit keystore, a self-signed certificate is generated
    using the identity established with:meth:`setDefaultIdentity(X500Principal) <.setDefaultIdentity>`, otherwise
    a default name is used.  The server may impose use restrictions when an automatic self-signed
    certificate is used and will cause client-side server-authentication issues if accessed
    remotely.  The OS-managed keystore is never used by the Ghidra Server.
    * Client - without an explicit keystore, the OS-managed user keystore is used
    when available (Windows-MY on Windows, Apple KeychainStore on
    macOS).
    
    
    
    NOTE: Since :obj:`SslRMIClientSocketFactory` and :obj:`SSLServerSocketFactory` employ a
    static cache of a default :obj:`SSLSocketFactory`, with its default :obj:`SSLContext`, we
    must utilize a wrapped implementation of the associated :obj:`X509ExtendedKeyManager` so that
    an updated keystore is used by the existing default :obj:`SSLSocketFactory`.
    """

    @typing.type_check_only
    class DefaultX509KeyManager(javax.net.ssl.X509ExtendedKeyManager):
        """
        ``DefaultKeyManager`` provides a wrapper for the X509 wrappedKeyManager whose
        instantiation is delayed until needed.  When a wrapper method is first invoked, the
        :meth:`DefaultX509KeyManager.init() <DefaultX509KeyManager.init>` method is called to open the keystore
        (which may require a password prompt) and establish the underlying X509KeyManager.
        """

        @typing.type_check_only
        class KeyManagerRecord(java.lang.Record):

            class_: typing.ClassVar[java.lang.Class]

            def equals(self, o: java.lang.Object) -> bool:
                ...

            def hashCode(self) -> int:
                ...

            def isOSManaged(self) -> bool:
                ...

            def isSelfSigned(self) -> bool:
                ...

            def keystorePath(self) -> str:
                ...

            def toString(self) -> str:
                ...

            def wrappedKeyManager(self) -> javax.net.ssl.X509KeyManager:
                ...

            @property
            def oSManaged(self) -> jpype.JBoolean:
                ...

            @property
            def selfSigned(self) -> jpype.JBoolean:
                ...


        class_: typing.ClassVar[java.lang.Class]


    class_: typing.ClassVar[java.lang.Class]
    KEYSTORE_PATH_PROPERTY: typing.Final = "ghidra.keystore"
    """
    Keystore path system property or user preference.  Setting the system
    property will take precedence over the user preference.
    """

    KEYSTORE_PASSWORD_PROPERTY: typing.Final = "ghidra.password"
    """
    Password system property may be set.  If set, this password will be used
    when accessing the keystore before attempting to use ``customPasswordProvider``
    if it has been set.
    """

    DEFAULT_PASSWORD: typing.Final = "changeme"

    def __init__(self) -> None:
        ...

    @staticmethod
    def addSubjectAlternativeName(subjectAltName: typing.Union[java.lang.String, str]) -> None:
        """
        Add the optional self-signed subject alternative name to be used during initialization
        if no keystore defined.  Current application key manager will be invalidated.
        (NOTE: this is intended for server use only when client will not be performing
        CA validation).
        
        :param java.lang.String or str subjectAltName: name to be added to the current list of alternative subject names.
        A null value will clear all names currently set.  
        name will be used to generate a self-signed certificate and private key
        """

    @staticmethod
    def getKeyManager() -> javax.net.ssl.X509ExtendedKeyManager:
        """
        Get the lazy default key manager associated with the preferred key store.
        
        :return: default key manager or null if not initialized
        :rtype: javax.net.ssl.X509ExtendedKeyManager
        """

    @staticmethod
    def getKeyStore() -> str:
        """
        Get the default/preferred key store file path.
        
        :return: default key store file path or null if not set
        :rtype: str
        """

    @staticmethod
    def getPreferredKeyStore() -> str:
        """
        If the system property *ghidra.keystore* takes precedence in establishing 
        the keystore.  If using a GUI and the system property has not been set, the 
        user preference with the same name will be used.
        
        :return: active keystore path or null if currently not running with a keystore or
        one has not been set.
        :rtype: str
        """

    @staticmethod
    def getSignedToken(authorities: jpype.JArray[java.security.Principal], token: jpype.JArray[jpype.JByte]) -> SignedToken:
        """
        Sign the supplied token byte array using an installed certificate from
        one of the specified authorities
        
        :param jpype.JArray[java.security.Principal] authorities: trusted certificate authorities used to constrain client certificate
        (may be null or empty array if CA constraint does not matter).
        :param jpype.JArray[jpype.JByte] token: token byte array
        :return: signed token object
        :rtype: SignedToken
        :raises NoSuchAlgorithmException: algorithm associated within signing certificate not found
        :raises SignatureException: failed to generate SignedToken
        :raises CertificateException: error associated with signing certificate
        """

    @staticmethod
    @typing.overload
    def initialize() -> bool:
        """
        Initialize key manager if needed.  Doing this explicitly independent of an SSL connection
        allows application to bail before initiating connection.  This will get handshake failure
        if user forgets keystore password or other keystore problem.
        
        
        The current role is retained (client mode unless :meth:`initialize(boolean) <.initialize>` was
        previously invoked with ``true``).  In client mode, when no keystore has been
        specified, the OS-managed keystore will be used if available.  Otherwise no keystore
        is used.
        
        :return: true if key manager initialized, otherwise false
        :rtype: bool
        """

    @staticmethod
    @typing.overload
    def initialize(isServer: typing.Union[jpype.JBoolean, bool]) -> bool:
        """
        Initialize key manager if needed, indicating whether this JVM is acting as a server
        or client.  In server mode the OS-managed keystore is never used.  If server mode and
        without an explicit keystore path, a self-signed certificate keystore will be generated 
        using the specified :meth:`identity <.setDefaultIdentity>` or a default identity.
        If the role differs from the current mode any existing key manager will be invalidated and 
        re-initialized.
        
        :param jpype.JBoolean or bool isServer: true if initializing for the Ghidra Server, false for client use.
        :return: true if key manager initialized, otherwise false
        :rtype: bool
        """

    @staticmethod
    def invalidateKeyManager() -> None:
        """
        Invalidate the existing default key manager.
        """

    @staticmethod
    def isMySignature(authorities: jpype.JArray[java.security.Principal], token: jpype.JArray[jpype.JByte], signature: jpype.JArray[jpype.JByte]) -> bool:
        """
        Verify that the specified sigBytes reflect my signature of the specified token.
        
        :param jpype.JArray[java.security.Principal] authorities: trusted certificate authorities used to constrain client certificate
        (may be null or empty array if CA constraint does not matter).
        :param jpype.JArray[jpype.JByte] token: byte array token
        :param jpype.JArray[jpype.JByte] signature: token signature
        :return: true if signature is my signature
        :rtype: bool
        :raises NoSuchAlgorithmException: algorithym associated within signing certificate not found
        :raises SignatureException: failed to generate SignedToken
        :raises CertificateException: error associated with signing certificate
        """

    @staticmethod
    def isServerMode() -> bool:
        """
        :return: true if key manager factory has been initialized for dedicated server use
        :rtype: bool
        """

    @staticmethod
    def setDefaultIdentity(identity: javax.security.auth.x500.X500Principal) -> None:
        """
        Set the default self-signed principal identity to be used during initialization
        if no keystore defined.  Current application key manager will be invalidated.
        (NOTE: this is intended for server use only when client will not be performing
        CA validation).
        
        :param javax.security.auth.x500.X500Principal identity: if not null and a KeyStore path has not be set, this
        identity will be used to generate a self-signed certificate and private key
        """

    @staticmethod
    def setDefaultKeyStore(path: typing.Union[java.lang.String, str], savePreference: typing.Union[jpype.JBoolean, bool]) -> bool:
        """
        Set default user keystore file path (e.g., certificate file with private key).
        This method will have no effect if the keystore had been set via the system
        property and an error will be displayed.  Otherwise, the keystore will
        be updated and the key manager re-initialized.  The user preference will be
        updated unless a failure occurred while attempting to open the keystore.
        This change will take immediate effect for the current executing application,
        however, it may still be superseded by a system property setting when running
        the application in the future. See :meth:`getKeyStore() <.getKeyStore>`.
        
        :param java.lang.String or str path: keystore file path or null to clear current key store and preference.
        :param jpype.JBoolean or bool savePreference: if true will be saved as user preference
        :return: true if successful else false if error occurred (see log).
        :rtype: bool
        """

    @staticmethod
    def usingFileKeyStore() -> bool:
        """
        Determine if active key manager is utilizing the file-based user keystore.
        
        :return: true if using the file-based user keystore.
        :rtype: bool
        """

    @staticmethod
    def usingGeneratedSelfSignedCertificate() -> bool:
        """
        Determine if active key manager is utilizing a generated self-signed certificate (server only)
        client certificate.
        
        :return: true if using self-signed certificate.
        :rtype: bool
        """

    @staticmethod
    def usingOSManagedKeyStore() -> bool:
        """
        Determine if active key manager is utilizing the OS-managed user keystore
        (``Windows-MY`` on Windows, Apple ``KeychainStore`` on macOS).
        
        :return: true if using the OS-managed keystore.
        :rtype: bool
        """


class SignedToken(java.lang.Object):
    """
    ``SignedToken`` provides the result of a signed token byte array.
    """

    class_: typing.ClassVar[java.lang.Class]
    token: typing.Final[jpype.JArray[jpype.JByte]]
    """
    Original token byte array
    """

    signature: typing.Final[jpype.JArray[jpype.JByte]]
    """
    Token byte array signature
    """

    algorithm: typing.Final[java.lang.String]
    """
    Algorithm used for signing
    """

    certChain: typing.Final[jpype.JArray[java.security.cert.X509Certificate]]
    """
    Certificate chain with the signing identity first, followed by its issuing CAs.
    The root certificate may be omitted.
    """



class DefaultSSLContextInitializer(ghidra.framework.ModuleInitializer):
    """
    Initialize the default SSLContext for use by all SSL connections (e.g., https).
    It is the responsibility of the Application to properly invoke this initializer 
    to ensure that the default SSLContext is properly established.
    
    
    The property ``jdk.tls.client.protocols`` should be set to restrict secure
    client connections to a specific set of enabled TLS protocols (e.g., TLSv1.2,TLSv1.3).
    See `JDK and JRE Cryptographic Algorithms <https://java.com/en/configure_crypto.html>`_ 
    for details.
    
    
    .. seealso::
    
        | :obj:`DefaultTrustManagerFactory`
    
        | :obj:`DefaultKeyManagerFactory`
    """

    class HttpsHostnameVerifier(javax.net.ssl.HostnameVerifier):
        """
        ``HttpsHostnameVerifier`` is required by HttpsURLConnection even
        if it does nothing.  The verify method will only be invoked if the default 
        behavior fails the connection attempt due to a hostname mismatch.
        
        
        .. seealso::
        
            | :obj:`HttpsURLConnection.setHostnameVerifier(HostnameVerifier)`
        """

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self) -> None:
            ...


    class_: typing.ClassVar[java.lang.Class]

    def __init__(self) -> None:
        ...

    @staticmethod
    @typing.overload
    def initialize(reset: typing.Union[jpype.JBoolean, bool]) -> bool:
        """
        Initialize default SSLContext with optional reset.
        This method is primarily intended for testing.
        
        :param jpype.JBoolean or bool reset: if true a complete reset will be done to force use of
        any new certificate or keystores previously used.
        :return: true if successful, else false (see logged error)
        :rtype: bool
        """

    @staticmethod
    @typing.overload
    def initialize() -> bool:
        """
        Initialize default SSLContext
        
        :return: true if successful, else false (see logged error)
        :rtype: bool
        """


class DefaultSSLSocketFactory(javax.net.ssl.SSLSocketFactory):
    """
    ``DefaultSSLSocketFactory`` provides a replacement for the default
    ``SSLSocketFactory`` which utilizes the default SSLContext established
    by :obj:`DefaultSSLContextInitializer`.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self) -> None:
        """
        ``DefaultSSLSocketFactory`` constructor.  
        SSLContext initialization will be performed using :obj:`DefaultSSLContextInitializer`.
        """


class DefaultTrustManagerFactory(java.lang.Object):
    """
    ``DefaultTrustManagerFactory`` provides the ability to establish
    acceptable certificate authorities to be used with the default SSLContext
    as established by :obj:`DefaultSSLContextInitializer`. 
    
    
    All SSL connections which leverage this factory will perform peer authentication against
    the established certificate authorities.  If an error occurs while reading a specified CA
    certs file, a "closed" trust policy is adopted and all peer authentication will fail.
    
    
    An application X.509 CA certificates file path may optionally be specified via the system
    property *ghidra.cacerts*.  The file may be in a standard form (*.pem, *.crt,
    *.cer, *.der) or may be in a Java JKS form (*.jks). The application may choose to set this
    property automatically based upon the presence of a *cacerts* file at a predetermined
    location prior to trust manager initialization.
    
    
    When the *ghidra.cacerts* property has been specified that trust store is used
    exclusively and the OS and Java default trust stores are ignored.  This applies to both
    client and server use, allowing a deployment to restrict trust to its own authorities.  When
    the property has not been specified the OS trust store and the Java default trust store are
    used.
    
    
    A server reached over a loopback connection is authenticated in the same way as any other.
    Setting the client property *ghidra.disable.loopback.server.authentication* to
    ``true`` waives that authentication for loopback connections only, which permits such a
    server to present a self-signed certificate.  It should be used only where every local account
    is trusted: a loopback connection is not inherently authentic, and any local process able to
    bind the port ahead of the intended server would be accepted in its place - along with any
    credential subsequently sent to it.
    
    
    NOTE: Since :obj:`SslRMIClientSocketFactory` and :obj:`SSLServerSocketFactory` employ a
    static cache of a default :obj:`SSLSocketFactory`, with its default :obj:`SSLContext`, we
    must utilize a wrapped implementation of the associated :obj:`X509TrustManager` so that any
    changes are used by the existing default :obj:`SSLSocketFactory`.
    """

    @typing.type_check_only
    class WrappedTrustManager(javax.net.ssl.X509ExtendedTrustManager):
        ...
        class_: typing.ClassVar[java.lang.Class]


    @typing.type_check_only
    class CertIdentity(java.lang.Object):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, cert: java.security.cert.X509Certificate) -> None:
            ...


    class_: typing.ClassVar[java.lang.Class]
    GHIDRA_CACERTS_PATH_PROPERTY: typing.Final = "ghidra.cacerts"
    """
    The VM property name to be used when specifying the application trust store
    X509 'cacerts' file path.
    """

    GHIDRA_DISABLE_LOOPBACK_SERVER_AUTH_PROPERTY: typing.Final = "ghidra.disable.loopback.server.authentication"
    """
    The VM property name which may be used to disable authentication of a server certificate
    presented over a loopback connection.  Loopback server authentication is performed by
    default; specifying this property as ``true`` waives it, permitting a server reached
    over loopback to present a self-signed (or otherwise untrusted) certificate.
    """


    @staticmethod
    def getTrustManagers() -> jpype.JArray[javax.net.ssl.TrustManager]:
        """
        Get trust manager after performing any necessary initialization.
        
        :return: trust managers
        :rtype: jpype.JArray[javax.net.ssl.TrustManager]
        """

    @staticmethod
    def getTrustedIssuers() -> jpype.JArray[javax.security.auth.x500.X500Principal]:
        """
        Returns a list of trusted issuers (i.e., CA certificates) as established
        by the :obj:`DefaultTrustManagerFactory`.
        
        :return: array of trusted Certificate Authorities
        :rtype: jpype.JArray[javax.security.auth.x500.X500Principal]
        :raises CertificateException: if failed to properly initialize trust manager
        due to CA certificate error(s).
        """

    @staticmethod
    def validateClient(certChain: jpype.JArray[java.security.cert.X509Certificate], authType: typing.Union[java.lang.String, str]) -> None:
        """
        Validate a client certificate ensuring that it is not expired and is
        trusted based upon the active trust managers.
        
        :param jpype.JArray[java.security.cert.X509Certificate] certChain: X509 certificate chain with the client identity first, followed by its
        issuing CAs; the root certificate may be omitted
        :param java.lang.String or str authType: authentication type (i.e., "RSA")
        :raises CertificateException: if certificate validation fails
        """


@typing.type_check_only
class FlexibleTrustStoreLoader(java.lang.Object):
    """
    Loads a trust :obj:`KeyStore` from a CA certificates file, which may be a Java key store
    (JKS or PKCS12) or a certificate file (one or more PEM encoded certificates, or a DER encoded
    certificate).
    
    
    The form of the file is determined from its content rather than by attempting each in turn, so
    that a failure to load can report why it failed instead of only that every attempt was
    unsuccessful.
    """

    class_: typing.ClassVar[java.lang.Class]

    @staticmethod
    def getTrustStore(cacertsPath: typing.Union[java.lang.String, str]) -> java.security.KeyStore:
        """
        Load a trust KeyStore from a Java key store (JKS, PKCS12) or a certificate file
        (PEM or DER).
        
        :param java.lang.String or str cacertsPath: path to CA certificates file
        :return: loaded certificates as keystore
        :rtype: java.security.KeyStore
        :raises KeyStoreException: if unable to load cacerts file
        :raises FileNotFoundException: cacerts file not found
        """


class HttpClients(java.lang.Object):

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self) -> None:
        ...

    @staticmethod
    def clearHttpClient() -> None:
        """
        Clears the currently cached :obj:`HttpClient`, forcing it to be
        rebuilt during the next call to :meth:`getHttpClient() <.getHttpClient>`.
        """

    @staticmethod
    def getHttpClient() -> java.net.http.HttpClient:
        """
        Returns a shared, plain (no special options) :obj:`HttpClient`.
        
        :return: a :obj:`HttpClient`
        :rtype: java.net.http.HttpClient
        :raises IOException: if error in PKI settings or crypto configuration
        """

    @staticmethod
    def newHttpClientBuilder() -> java.net.http.HttpClient.Builder:
        """
        Creates a HttpClient Builder using Ghidra SSL/TLS context info.
        
        :return: a new HttpClient Builder
        :rtype: java.net.http.HttpClient.Builder
        :raises IOException: if error in PKI settings or crypto configuration
        """


class NetworkUtils(java.lang.Object):
    """
    :obj:`NetworkUtils` provides collection of network utilities
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self) -> None:
        ...

    @staticmethod
    def isLoopbackAddress(ipAddress: typing.Union[java.lang.String, str]) -> bool:
        """
        :return: true if specified ipAddress corresponds to a loopback hostname of address
        :rtype: bool
        
        NOTE: The presence of any trailing CIDR netmask on an IPv4 or IPv6 address is ignored.
        
        :param java.lang.String or str ipAddress: IP address or hostname (e.g., "127.0.0.1", "localhost")
        """


class PKIUtils(java.lang.Object):
    """
    :obj:`PKIUtils` provides supporting utilities for creating and accessing X509 certificate
    keystore files.
    """

    class_: typing.ClassVar[java.lang.Class]
    RSA_TYPE: typing.Final = "RSA"
    SIGNING_ALGORITHM: typing.Final = "SHA512withRSA"
    KEY_SIZE: typing.Final = 4096
    BEGIN_CERT: typing.Final = "-----BEGIN CERTIFICATE-----"
    END_CERT: typing.Final = "-----END CERTIFICATE-----"
    JKS_TYPE: typing.Final = "JKS"
    """
    Java key store type of a JKS key store (see :meth:`detectKeyStoreType(String) <.detectKeyStoreType>`)
    """

    PKCS12_TYPE: typing.Final = "PKCS12"
    """
    Java key store type of a PKCS#12 key store (see :meth:`detectKeyStoreType(String) <.detectKeyStoreType>`)
    """

    PKCS_FILE_EXTENSIONS: typing.Final[jpype.JArray[java.lang.String]]
    PKCS_FILENAME_FILTER: typing.Final[javax.swing.filechooser.FileNameExtensionFilter]

    @staticmethod
    def createKeyStore(alias: typing.Union[java.lang.String, str], dn: typing.Union[java.lang.String, str], durationDays: typing.Union[jpype.JInt, int], caEntry: java.security.KeyStore.PrivateKeyEntry, isCA: typing.Union[jpype.JBoolean, bool], keyFile: jpype.protocol.SupportsPath, keystoreType: typing.Union[java.lang.String, str], subjectAlternativeNames: collections.abc.Sequence, protectedPassphrase: jpype.JArray[jpype.JChar]) -> java.security.KeyStore:
        """
        Generate a new :obj:`X509Certificate` with RSA :obj:`KeyPair` and create/update a 
        :obj:`KeyStore` optionally backed by a keyFile. 
        
        
        Standard usage will include :obj:`KeyUsage.digitalSignature` and :obj:`KeyUsage.keyEncipherment`.
        For non-CA extended usage will include serverAuth and clientAuth for maximum compatibility.
        
        :param java.lang.String or str alias: entry alias with keystore
        :param java.lang.String or str dn: distinguished name (e.g., "CN=Ghidra Test, O=Ghidra, OU=Test, C=US" )
        :param jpype.JInt or int durationDays: number of days which generated certificate should remain valid
        :param java.security.KeyStore.PrivateKeyEntry caEntry: optional CA private key entry (issuer of new certificate).  If null, new 
                certificate will sign itself (self-signed).
        :param jpype.JBoolean or bool isCA: if true the new certificate will tagged as a CA
        :param jpype.protocol.SupportsPath keyFile: optional file to load/store resulting :obj:`KeyStore` (may be null)
        :param java.lang.String or str keystoreType: support keystore type (e.g., "JKS", "PKCS12")
        :param collections.abc.Sequence subjectAlternativeNames: an optional list of subject alternative names to be included 
                    in certificate (may be null)
        :param jpype.JArray[jpype.JChar] protectedPassphrase: key and keystore protection password
        :return: keystore containing newly generated certification with key pair
        :rtype: java.security.KeyStore
        :raises KeyStoreException: if error occurs while updating keystore
        """

    @staticmethod
    def detectKeyStoreType(keystorePath: typing.Union[java.lang.String, str]) -> str:
        """
        Determine the Java key store type of a file from its leading bytes.
        
        
        Only the Java key store forms are identified.  A certificate file is not a key store and
        yields null, which includes a **DER** encoded certificate: a DER certificate and a PKCS#12
        key store cannot be told apart by their leading byte, since both begin with an ASN.1 DER
        SEQUENCE (0x30).  They are distinguished here by the first element *within* that
        SEQUENCE, which is an INTEGER version for a PKCS#12 PFX and a nested SEQUENCE
        (tbsCertificate) for a certificate:
        
        PFX         ::= SEQUENCE { version INTEGER {v3(3)}, authSafe ContentInfo, ... }
        Certificate ::= SEQUENCE { tbsCertificate TBSCertificate, ... }  -- itself a SEQUENCE
        
        Locating that element requires the SEQUENCE length to be decoded, since it may use the DER
        long form (the Java default *cacerts* does).
        
        
        This remains a determination based upon the leading bytes and not a full parse, so a caller
        must still be prepared for a key store of the identified type to fail to load.
        
        :param java.lang.String or str keystorePath: path to the file to be examined
        :return: JKS or PKCS12 if the file is a Java key store of that
        type, otherwise null (a PEM or DER certificate file among the possibilities)
        :rtype: str
        :raises IOException: if the file cannot be read
        """

    @staticmethod
    def exportX509Certificates(certificates: jpype.JArray[java.security.cert.Certificate], outFile: jpype.protocol.SupportsPath) -> None:
        """
        Export X.509 certificates to the specified outFile.
        
        :param jpype.JArray[java.security.cert.Certificate] certificates: certificates to be stored
        :param jpype.protocol.SupportsPath outFile: output file
        :raises IOException: if error occurs writing to outFile
        :raises CertificateEncodingException: if error occurs while encoding certificate data
        """

    @staticmethod
    def formatKeyUsage(cert: java.security.cert.X509Certificate) -> str:
        """
        :return: the key-usages as sequence of comma-separated names for a specified certificate
        :rtype: str
        
        
        :param java.security.cert.X509Certificate cert: x509 certificate
        """

    @staticmethod
    @typing.overload
    def getCommonName(cert: java.security.cert.X509Certificate) -> str:
        """
        :return: the Common Name (CN) for an X509 certificate or null if DN does not contain a CN
        :rtype: str
        
        
        :param java.security.cert.X509Certificate cert: X509 certificate
        :raises InvalidNameException: if DN is invalid or contains invalid DN
        """

    @staticmethod
    @typing.overload
    def getCommonName(distinguishedName: typing.Union[java.lang.String, str]) -> str:
        """
        :return: the Common Name (CN) for an X509 distinguished name (DN) or null if DN does not contain a CN
        :rtype: str
        
        
        :param java.lang.String or str distinguishedName: distinguished name (DN)
        :raises InvalidNameException: if DN is invalid or contains invalid CN
        """

    @staticmethod
    def getKeyStoreInstance(keystorePath: typing.Union[java.lang.String, str], password: jpype.JArray[jpype.JChar]) -> java.security.KeyStore:
        """
        Attempt to load a client/server keystore in a PKCS12 form (*.p12, *.pks, *.pfx) or 
        Java JKS (*.jks) form.
        
        :param java.lang.String or str keystorePath: JKS or PKCS12 keystore file path
        :param jpype.JArray[jpype.JChar] password: keystore password
        :return: keystore instance
        :rtype: java.security.KeyStore
        :raises IOException: if failure occurred reading and processing keystore file or if the 
                    given password was incorrect. If the error is due to a wrong password, the 
                    :obj:`cause <Throwable.getCause>` of the ``IOException`` should be an
                    ``UnrecoverableKeyException``
        :raises NoSuchAlgorithmException: if the algorithm used to check the integrity of the 
                    keystore cannot be found
        :raises CertificateException: if any of the certificates in the keystore could not be loaded
        :raises KeyStoreException: if a general error occurred opening/processing keystore
        """

    @staticmethod
    def getTrustManager(caCertsFile: jpype.protocol.SupportsPath) -> javax.net.ssl.X509TrustManager:
        """
        Establish X509TrustManager for the specified CA certificate storage.
        
        :param jpype.protocol.SupportsPath caCertsFile: CA certificates storage file
        :return: X509TrustManager
        :rtype: javax.net.ssl.X509TrustManager
        :raises GeneralSecurityException: if error occured during truststore initialization
        :raises IOException: if file read error occurs
        """

    @staticmethod
    def loadCertificateStore(certsPath: typing.Union[java.lang.String, str]) -> java.security.KeyStore:
        """
        Load the all certificates from the specified certificate store in a standard
        X.509 form (e.g., concatenation of Base64 encoded certificates: *.pem, *.crt, *.cer, *.der) 
        or Java JKS (*.jks) form.
        
        :param java.lang.String or str certsPath: certificate(s) storage file path
        :return: KeyStore containing certificates
        :rtype: java.security.KeyStore
        :raises IOException: if failure occurred reading and processing keystore file.
        :raises NoSuchAlgorithmException: if the algorithm used to check the integrity of the 
                    keystore cannot be found
        :raises CertificateException: if any of the certificates in the keystore could not be loaded
        :raises KeyStoreException: if a general error occurred opening/processing keystore
        """

    @staticmethod
    def loadX509CertificateStore(certsPath: typing.Union[java.lang.String, str], certConsumer: utility.function.ExceptionalConsumer[java.security.cert.X509Certificate, java.security.KeyStoreException]) -> int:
        """
        Load the all certificates from the specified certificate store in a standard
        X.509 form (e.g., concatenation of Base64 encoded certificates: *.pem, *.crt, *.cer, *.der)
        
        :param java.lang.String or str certsPath: certificate(s) storage file path
        :param utility.function.ExceptionalConsumer[java.security.cert.X509Certificate, java.security.KeyStoreException] certConsumer: consumer callback to be supplied with each certificate
        :return: number of X509 certificates read
        :rtype: int
        :raises IOException: if failure occurred reading and processing keystore file.
        :raises CertificateException: if any of the certificates in the keystore could not be loaded
        :raises KeyStoreException: thrown by certConsumer
        """

    @staticmethod
    @typing.overload
    def loadX509PemCertificates(pemFile: jpype.protocol.SupportsPath) -> java.util.List[java.security.cert.X509Certificate]:
        """
        Load all X.509 certificates from an unencrypted PEM file (a concatenation of Base64 encoded
        certificates).  Text surrounding the certificate blocks, such as the certificate details which
        some tools emit ahead of each one, is ignored.
        
        
        Unlike :meth:`loadCertificateStore(String) <.loadCertificateStore>` this requires the file to be PEM encoded and to
        contain nothing but complete certificates, as a certificate authority file must (e.g., the
        PostgreSQL ``ssl_ca_file``) since every entry within it has to be usable: a DER encoded
        certificate or a keystore yields no certificates rather than being accepted, and a truncated or
        non-certificate PEM block is an error rather than being skipped.
        
        :param jpype.protocol.SupportsPath pemFile: PEM certificate file
        :return: the certificates in the order they occur within the file, which is empty if the file
                    contains no PEM block at all
        :rtype: java.util.List[java.security.cert.X509Certificate]
        :raises IOException: if the file cannot be read or a certificate block is truncated
        :raises CertificateException: if a PEM block is not a certificate or cannot be decoded
        """

    @staticmethod
    @typing.overload
    def loadX509PemCertificates(pemFile: jpype.protocol.SupportsPath, unusableBlockConsumer: java.util.function.Consumer[java.lang.String]) -> java.util.List[java.security.cert.X509Certificate]:
        """
        Read all X509 certificates contained within a PEM encoded file, optionally tolerating any
        block which cannot be used.
        
        
        Tolerating unusable blocks is appropriate when establishing a trust store, where discarding
        every valid certificate on account of one defective block would be the greater failure.  It
        is not appropriate when validating a file which the user has supplied for that purpose, where
        a defect must be reported rather than passed over - such callers use
        :meth:`loadX509PemCertificates(File) <.loadX509PemCertificates>` instead.
        
        :param jpype.protocol.SupportsPath pemFile: PEM encoded certificate file
        :param java.util.function.Consumer[java.lang.String] unusableBlockConsumer: if non-null, a description of each unusable PEM block is passed
        to this consumer and the block is skipped, allowing the remaining certificates to be read.
        If null, the first unusable block is an error.
        :return: list of X509 certificates read from the file
        :rtype: java.util.List[java.security.cert.X509Certificate]
        :raises IOException: if the file cannot be read
        :raises CertificateException: if a block cannot be used and unusableBlockConsumer is null
        """

    @staticmethod
    def logCerts(x509Certs: jpype.JArray[java.security.cert.X509Certificate]) -> None:
        """
        Log all X509 certificates contained within array
        
        :param jpype.JArray[java.security.cert.X509Certificate] x509Certs: array of certificates
        """

    @staticmethod
    def saveKeyStore(keyStore: java.security.KeyStore, keyFile: jpype.protocol.SupportsPath, protectedPassphrase: jpype.JArray[jpype.JChar]) -> None:
        """
        Save a keystore
        
        :param java.security.KeyStore keyStore: key store to be saved
        :param jpype.protocol.SupportsPath keyFile: file to be written
        :param jpype.JArray[jpype.JChar] protectedPassphrase: protection password
        :raises KeyStoreException: if error occurred while generating keystore
        :raises IOException: if failed to write keystore file
        """


class ServerCertificateException(java.security.cert.CertificateException):
    """
    :obj:`ServerCertificateException` is thrown when TLS/SSL server
    certificate validation fails during an attempted server connection.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, serverCertChain: jpype.JArray[java.security.cert.X509Certificate], cause: java.security.cert.CertificateException) -> None:
        """
        Constructor
        
        :param jpype.JArray[java.security.cert.X509Certificate] serverCertChain: TLS peer certificate chain, with the server identity first
        :param java.security.cert.CertificateException cause: underlying validation cause
        """

    def getServerCertChain(self) -> jpype.JArray[java.security.cert.X509Certificate]:
        """
        :return: server certificate chain
        :rtype: jpype.JArray[java.security.cert.X509Certificate]
        """

    @property
    def serverCertChain(self) -> jpype.JArray[java.security.cert.X509Certificate]:
        ...


class UnixSystemTrustKeyStoreUtil(java.lang.Object):
    """
    Builds a trust :obj:`KeyStore` backed by the host Linux (and BSD variants) operating system's 
    known CA trust-store locations. 
    
    
    Rationale: Java ships no native "OS trust store" KeyStore provider on
    Linux/BSD (unlike "Windows-ROOT" on Windows or "KeychainStore" on macOS).
    Nearly every distro instead maintains one or more PEM bundle files (and/or
    a directory of individual PEM certs) kept in sync with the system's trust
    configuration by tooling such as update-ca-certificates, update-ca-trust,
    or p11-kit. This class scans the well-known locations for those bundles,
    parses every certificate it finds, de-duplicates by fingerprint, and loads
    the result into an in-memory KeyStore that is handed to a
    TrustManagerFactory.
    
    
    Desktop environments (GNOME, KDE, XFCE, Cinnamon, etc.) don't keep a
    separate trust store for TLS -- they consume the same system bundle
    (often via p11-kit / NSS, which is itself synced from the paths below), so
    no desktop-manager-specific handling is required beyond this.
    
    
    **Only the trust configuration the OS has actually put into effect is used.** The
    locations scanned are those which the OS tooling *produces* -- the extracted bundles of
    :obj:`.BUNDLE_FILES` and the enabled-certificate directories of :obj:`.BUNDLE_DIRS` -- and
    never the source/anchor directories which feed that tooling. Those source directories retain
    certificates which an administrator has since disabled: Debian and Ubuntu disable a CA by
    prefixing it with ``!`` in ``/etc/ca-certificates.conf`` (the file itself remaining
    under ``/usr/share/ca-certificates``), and RHEL and Fedora distrust one through
    ``/etc/pki/ca-trust/source/blocklist`` while it may still sit in
    ``source/anchors``. Reading a source directory would therefore re-trust exactly those
    authorities the administrator removed, so it is not done.
    
    
    Known limitations:
    
    * Requires read access to the relevant system paths (usually
    world-readable, but not guaranteed in hardened/minimal images).
    * Minimal container images (e.g. distroless, scratch) may not have any
    CA bundle installed at all -- no OS trust store is produced in that case
    and the caller falls back to the Java default trust store.
    * Distros that route trust exclusively through a PKCS#11/NSS token
    rather than exporting a PEM bundle are not covered; nearly all
    mainstream distros do export a PEM bundle as of this writing.
    """

    class_: typing.ClassVar[java.lang.Class]



__all__ = ["ApplicationKeyManagerFactory", "CertTool", "DefaultKeyManagerFactory", "SignedToken", "DefaultSSLContextInitializer", "DefaultSSLSocketFactory", "DefaultTrustManagerFactory", "FlexibleTrustStoreLoader", "HttpClients", "NetworkUtils", "PKIUtils", "ServerCertificateException", "UnixSystemTrustKeyStoreUtil"]
