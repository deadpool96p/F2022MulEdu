from __future__ import annotations
import collections.abc
import datetime
import typing
from warnings import deprecated # type: ignore

import jpype # type: ignore
import jpype.protocol # type: ignore

import java.lang # type: ignore


class CertificateManagerLauncher(java.lang.Object):
    """
    Launches (or refocuses) the OS-native certificate manager used to view/manage the
    user's trusted CA certificates — the same tool Chrome/Edge expose via their own
    "Manage certificates" button. Supported on Windows and macOS only.
    
    
    Windows: displays the native ``CryptUIDlgCertMgr`` dialog via :obj:`WindowsCertMgrApp`,
    launched as an independent Java process (that call blocks until the dialog is closed, so
    it must not run inside Ghidra's own JVM). A subsequent call while that process is still
    alive brings its window to the foreground instead of starting a second one.
    
    
    macOS: launches Keychain Access via ``open -a``, which itself focuses an
    already-running instance instead of starting a duplicate.
    """

    @typing.type_check_only
    class Win32(java.lang.Object):
        """
        Win32 bindings used to locate and focus an existing certificate manager window.
        
        
        These are established within a nested class so that they are initialized when first used,
        which only occurs on Windows.  Establishing them within :obj:`CertificateManagerLauncher`
        itself would fail on every other platform and would prevent the macOS branch of
        :meth:`launchOrFocus() <.launchOrFocus>` from running.
        """

        class_: typing.ClassVar[java.lang.Class]


    class_: typing.ClassVar[java.lang.Class]

    @staticmethod
    def launchOrFocus() -> None:
        """
        Launch the platform certificate manager, or bring an already-launched instance to the
        foreground. Safe to call repeatedly/rapidly from the Swing thread; a single
        ``synchronized`` entry point is sufficient serialization since every operation
        performed here (starting a process, enumerating windows) returns quickly.
        """


class WindowsCertMgrApp(java.lang.Object):
    """
    Standalone helper application which displays the Windows-native certificate manager
    dialog via the ``CryptUIDlgCertMgr`` Win32 API using Project Panama (FFM API).
    """

    class_: typing.ClassVar[java.lang.Class]

    @staticmethod
    def main(args: jpype.JArray[java.lang.String]) -> None:
        ...



__all__ = ["CertificateManagerLauncher", "WindowsCertMgrApp"]
