from __future__ import annotations
import collections.abc
import datetime
import typing
from warnings import deprecated # type: ignore

import jpype # type: ignore
import jpype.protocol # type: ignore

import docking
import docking.action
import docking.widgets.table
import ghidra.app.plugin.core.data
import ghidra.app.plugin.processors.sleigh
import ghidra.base.widgets.table
import ghidra.debug.api.tracemgr
import ghidra.docking.settings
import ghidra.framework.plugintool
import ghidra.pcode.emu
import ghidra.pcode.exec_
import ghidra.program.model.address
import ghidra.program.model.data
import ghidra.program.model.lang
import ghidra.program.model.listing
import ghidra.program.model.pcode
import ghidra.trace.model
import ghidra.util.task
import java.lang # type: ignore
import java.util # type: ignore
import javax.swing # type: ignore


C = typing.TypeVar("C")
R = typing.TypeVar("R")
T = typing.TypeVar("T")


class AddressInputVerifier(javax.swing.InputVerifier):

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, factory: ghidra.program.model.address.AddressFactory) -> None:
        ...


class LocAndVal(java.lang.Record):

    class LocAndValPcodeArithmetic(java.lang.Enum[LocAndVal.LocAndValPcodeArithmetic], ghidra.pcode.exec_.PcodeArithmetic[LocAndVal]):

        class_: typing.ClassVar[java.lang.Class]
        BIG_ENDIAN: typing.Final[LocAndVal.LocAndValPcodeArithmetic]
        LITTLE_ENDIAN: typing.Final[LocAndVal.LocAndValPcodeArithmetic]

        @staticmethod
        def forEndian(isBigEndian: typing.Union[jpype.JBoolean, bool]) -> LocAndVal.LocAndValPcodeArithmetic:
            ...

        @staticmethod
        def forLanguage(language: ghidra.program.model.lang.Language) -> LocAndVal.LocAndValPcodeArithmetic:
            ...

        @staticmethod
        def valueOf(name: typing.Union[java.lang.String, str]) -> LocAndVal.LocAndValPcodeArithmetic:
            ...

        @staticmethod
        def values() -> jpype.JArray[LocAndVal.LocAndValPcodeArithmetic]:
            ...


    class LocAndValPcodeExecutorStatePiece(ghidra.pcode.exec_.PcodeExecutorStatePiece[jpype.JArray[jpype.JByte], LocAndVal]):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, bytesPiece: ghidra.pcode.exec_.PcodeExecutorStatePiece[jpype.JArray[jpype.JByte], jpype.JArray[jpype.JByte]], locationPiece: ghidra.pcode.exec_.PcodeExecutorStatePiece[jpype.JArray[jpype.JByte], ghidra.pcode.exec_.ValueLocation]) -> None:
            ...


    class LocAndValPcodeExecutorState(ghidra.pcode.exec_.AbstractPcodeExecutorState[jpype.JArray[jpype.JByte], LocAndVal]):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, piece: ghidra.pcode.exec_.PcodeExecutorStatePiece[jpype.JArray[jpype.JByte], LocAndVal]) -> None:
            ...


    class WriterAndState(java.lang.Record):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, writer: ghidra.pcode.exec_.trace.TraceEmulationIntegration.Writer, state: LocAndVal.LocAndValPcodeExecutorState) -> None:
            ...

        def equals(self, o: java.lang.Object) -> bool:
            ...

        def hashCode(self) -> int:
            ...

        def state(self) -> LocAndVal.LocAndValPcodeExecutorState:
            ...

        def toString(self) -> str:
            ...

        def writer(self) -> ghidra.pcode.exec_.trace.TraceEmulationIntegration.Writer:
            ...


    class WriterAndExecutor(java.lang.Record):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, writer: ghidra.pcode.exec_.trace.TraceEmulationIntegration.Writer, executor: ghidra.pcode.exec_.PcodeExecutor[LocAndVal]) -> None:
            ...

        def equals(self, o: java.lang.Object) -> bool:
            ...

        def executor(self) -> ghidra.pcode.exec_.PcodeExecutor[LocAndVal]:
            ...

        def hashCode(self) -> int:
            ...

        def toString(self) -> str:
            ...

        def writer(self) -> ghidra.pcode.exec_.trace.TraceEmulationIntegration.Writer:
            ...


    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, value: jpype.JArray[jpype.JByte], loc: ghidra.pcode.exec_.ValueLocation) -> None:
        ...

    @staticmethod
    def buildExecutor(provider: ghidra.framework.plugintool.ServiceProvider, coordinates: ghidra.debug.api.tracemgr.DebuggerCoordinates) -> LocAndVal.WriterAndExecutor:
        ...

    @staticmethod
    def buildState(provider: ghidra.framework.plugintool.ServiceProvider, coordinates: ghidra.debug.api.tracemgr.DebuggerCoordinates) -> LocAndVal.WriterAndState:
        ...

    def equals(self, o: java.lang.Object) -> bool:
        ...

    def hashCode(self) -> int:
        ...

    def loc(self) -> ghidra.pcode.exec_.ValueLocation:
        ...

    def toString(self) -> str:
        ...

    def value(self) -> jpype.JArray[jpype.JByte]:
        ...


class FunctionEmulationHarness(java.lang.AutoCloseable):

    class ProbeOut(java.lang.Record):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, vn: ghidra.program.model.pcode.Varnode, value: jpype.JArray[jpype.JByte]) -> None:
            ...

        def equals(self, o: java.lang.Object) -> bool:
            ...

        def hashCode(self) -> int:
            ...

        @typing.overload
        def toString(self, language: ghidra.program.model.lang.Language) -> str:
            ...

        @typing.overload
        def toString(self) -> str:
            ...

        def value(self) -> jpype.JArray[jpype.JByte]:
            ...

        def vn(self) -> ghidra.program.model.pcode.Varnode:
            ...


    class ReturnAddressInfo(java.lang.Record):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, location: ghidra.program.model.address.Address, mask: typing.Union[jpype.JLong, int]) -> None:
            ...

        def computePhysicalLocation(self, eval: FunctionEmulationHarness.Eval, cSpec: ghidra.program.model.lang.CompilerSpec) -> ghidra.program.model.address.Address:
            ...

        def equals(self, o: java.lang.Object) -> bool:
            ...

        def hashCode(self) -> int:
            ...

        def location(self) -> ghidra.program.model.address.Address:
            ...

        def mask(self) -> int:
            ...

        def toString(self) -> str:
            ...


    class Eval(java.lang.Object):

        class_: typing.ClassVar[java.lang.Class]
        snap: typing.Final[jpype.JLong]
        writer: typing.Final[ghidra.pcode.exec_.trace.TraceEmulationIntegration.Writer]
        exec_: typing.Final[ghidra.pcode.exec_.PcodeExecutor[LocAndVal]]
        state: typing.Final[ghidra.pcode.exec_.PcodeExecutorState[LocAndVal]]

        def commit(self) -> None:
            ...

        def readNode(self, node: VarStorageNode) -> LocAndVal:
            ...

        def readVariable(self, storage: VarStorage) -> LocAndVal:
            ...

        def writeNode(self, node: VarStorageNode, value: LocAndVal) -> None:
            ...

        @typing.overload
        def writeVariable(self, storage: VarStorage, value: LocAndVal) -> None:
            ...

        @typing.overload
        def writeVariable(self, storage: VarStorage, value: jpype.JArray[jpype.JByte]) -> None:
            ...


    @typing.type_check_only
    class MonitoredCallbacks(ghidra.pcode.emu.ComposedPcodeEmulationCallbacks[jpype.JArray[jpype.JByte]]):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, cb: ghidra.pcode.emu.PcodeEmulationCallbacks[jpype.JArray[jpype.JByte]]) -> None:
            ...


    class OverrideProbeUseropLibrary(ghidra.pcode.exec_.AnnotatedPcodeUseropLibrary[jpype.JArray[jpype.JByte]]):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self) -> None:
            ...

        def emu_probe(self, exec_: ghidra.pcode.exec_.PcodeExecutor[jpype.JArray[jpype.JByte]], in_: ghidra.program.model.pcode.Varnode) -> None:
            ...


    @typing.type_check_only
    class MonitoredEmulator(ghidra.pcode.emu.PcodeEmulator):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, language: ghidra.program.model.lang.Language, cb: ghidra.pcode.emu.PcodeEmulationCallbacks[jpype.JArray[jpype.JByte]]) -> None:
            ...


    class EmulationResult(java.lang.Record):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, snap: typing.Union[jpype.JLong, int], error: java.lang.Throwable) -> None:
            ...

        def defaultSnap(self) -> int:
            """
            In the case we terminate normally, we should probably ensure the ``return``
            instruction highlighted. This can be accomplished by subtracting one from the last snap.
            For abnormal termination, I think we should just go to the last snap and let whatever
            problem(s) get displayed.
            
            :return: the last snap - 1 if successful, otherwise just the last snap.
            :rtype: int
            """

        def equals(self, o: java.lang.Object) -> bool:
            ...

        def error(self) -> java.lang.Throwable:
            ...

        def hashCode(self) -> int:
            ...

        def snap(self) -> int:
            ...

        def toString(self) -> str:
            ...


    class_: typing.ClassVar[java.lang.Class]
    trace: typing.Final[ghidra.trace.model.Trace]
    lvArith: typing.Final[ghidra.pcode.exec_.PcodeArithmetic[LocAndVal]]

    def createSnapshot(self, snap: typing.Union[jpype.JLong, int]) -> None:
        ...

    def eval(self, snap: typing.Union[jpype.JLong, int]) -> FunctionEmulationHarness.Eval:
        ...

    def getProbesOut(self) -> java.util.List[FunctionEmulationHarness.ProbeOut]:
        ...

    def init(self) -> FunctionEmulationHarness.Eval:
        ...

    def installInjects(self) -> None:
        ...

    def locateReturnAddress(self) -> FunctionEmulationHarness.ReturnAddressInfo:
        ...

    def placeSentinel(self, sentinel: ghidra.program.model.address.Address) -> None:
        ...

    def run(self, snapshotPeriod: typing.Union[jpype.JLong, int]) -> FunctionEmulationHarness.EmulationResult:
        ...

    @staticmethod
    def start(tool: ghidra.framework.plugintool.PluginTool, function: ghidra.program.model.listing.Function, monitor: ghidra.util.task.TaskMonitor) -> FunctionEmulationHarness:
        ...

    @property
    def probesOut(self) -> java.util.List[FunctionEmulationHarness.ProbeOut]:
        ...


class DebuggerEmulateFunctionDialog(docking.DialogComponentProvider):

    @typing.type_check_only
    class VarActionContext(docking.DefaultActionContext, typing.Generic[T]):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, selection: java.util.List[T]) -> None:
            ...

        def getSelection(self) -> java.util.List[T]:
            ...

        @property
        def selection(self) -> java.util.List[T]:
            ...


    @typing.type_check_only
    class InputActionContext(DebuggerEmulateFunctionDialog.VarActionContext[InputRow]):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, selection: java.util.List[InputRow]) -> None:
            ...


    @typing.type_check_only
    class OutputActionContext(DebuggerEmulateFunctionDialog.VarActionContext[OutputRow]):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, selection: java.util.List[OutputRow]) -> None:
            ...


    @typing.type_check_only
    class ActionContainer(docking.action.ActionContextProvider):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, container: javax.swing.JComponent) -> None:
            ...

        def addAction(self, action: docking.action.DockingActionIf) -> None:
            ...

        def contextChanged(self) -> None:
            ...


    @typing.type_check_only
    class HorizontalBoxActionContainer(DebuggerEmulateFunctionDialog.ActionContainer):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self) -> None:
            ...


    @typing.type_check_only
    class Allocator(java.lang.Object):

        class_: typing.ClassVar[java.lang.Class]

        def allocate(self, length: typing.Union[jpype.JInt, int]) -> None:
            ...


    @typing.type_check_only
    class RowConstructor(java.lang.Object, typing.Generic[T]):

        class_: typing.ClassVar[java.lang.Class]

        def create(self, name: typing.Union[java.lang.String, str], storage: VarStorage, type: ghidra.program.model.data.DataType) -> T:
            ...


    @typing.type_check_only
    class VarAdder(java.lang.Object, typing.Generic[T]):

        @typing.type_check_only
        class SettingsAndValue(java.lang.Record):

            class_: typing.ClassVar[java.lang.Class]

            def encoded(self) -> jpype.JArray[jpype.JByte]:
                ...

            def equals(self, o: java.lang.Object) -> bool:
                ...

            def hashCode(self) -> int:
                ...

            def settings(self) -> ghidra.docking.settings.Settings:
                ...

            def toString(self) -> str:
                ...


        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, from_: T, into: docking.widgets.table.DefaultEnumeratedColumnTableModel[typing.Any, T], added: java.util.List[T]) -> None:
            ...

        def addPointerVars(self, count: typing.Union[jpype.JInt, int]) -> None:
            ...

        def addStringVar(self) -> None:
            ...


    @typing.type_check_only
    class InputAdder(DebuggerEmulateFunctionDialog.VarAdder[InputRow]):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, from_: InputRow, added: java.util.List[InputRow], settingsMap: collections.abc.Mapping) -> None:
            ...


    @typing.type_check_only
    class OutputAdder(DebuggerEmulateFunctionDialog.VarAdder[OutputRow]):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, from_: OutputRow, added: java.util.List[OutputRow]) -> None:
            ...


    @typing.type_check_only
    class VarDataSettingsDialog(ghidra.app.plugin.core.data.AbstractSettingsDialog):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, type: ghidra.program.model.data.DataType, mySettings: ghidra.docking.settings.Settings) -> None:
            ...


    @typing.type_check_only
    class InputInitializer(java.lang.Object):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, eval: FunctionEmulationHarness.Eval, inputs: collections.abc.Sequence) -> None:
            ...

        def checkConflicts(self) -> None:
            ...

        def doInitialize(self, input: InputRow) -> None:
            ...

        def initializeDependencies(self, input: InputRow) -> None:
            ...

        def initializeDependency(self, name: typing.Union[java.lang.String, str]) -> None:
            ...

        def initializeInput(self, input: InputRow) -> None:
            ...

        def initializeInputs(self) -> None:
            ...

        def toHex(self, value: jpype.JArray[jpype.JByte], length: typing.Union[jpype.JInt, int]) -> str:
            ...


    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, tool: ghidra.framework.plugintool.PluginTool, function: ghidra.program.model.listing.Function) -> None:
        ...


@typing.type_check_only
class InputsTableModel(VarTableModel[InputColumn, InputRow]):

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, tool: ghidra.framework.plugintool.ServiceProvider, dialog: DebuggerEmulateFunctionDialog) -> None:
        ...


@typing.type_check_only
class VarTableModel(docking.widgets.table.DefaultEnumeratedColumnTableModel[C, R], typing.Generic[C, R]):

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, tool: ghidra.framework.plugintool.ServiceProvider, name: typing.Union[java.lang.String, str], colType: java.lang.Class[C], dialog: DebuggerEmulateFunctionDialog) -> None:
        ...


@typing.type_check_only
class InputColumn(java.lang.Enum[InputColumn], docking.widgets.table.EnumeratedTableColumn[InputColumn, InputRow]):

    class_: typing.ClassVar[java.lang.Class]
    NAME: typing.Final[InputColumn]
    STORAGE: typing.Final[InputColumn]
    VALUE: typing.Final[InputColumn]
    TYPE: typing.Final[InputColumn]
    REPR: typing.Final[InputColumn]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> InputColumn:
        ...

    @staticmethod
    def values() -> jpype.JArray[InputColumn]:
        ...


@typing.type_check_only
class InputRow(VarRow):
    ...
    class_: typing.ClassVar[java.lang.Class]


@typing.type_check_only
class VarRow(java.lang.Object):

    @typing.type_check_only
    class VarRowFactory(java.lang.Object, typing.Generic[T]):

        class_: typing.ClassVar[java.lang.Class]

        def create(self, language: ghidra.program.model.lang.Language, name: typing.Union[java.lang.String, str], storage: VarStorage, type: ghidra.program.model.data.DataType) -> T:
            ...


    class_: typing.ClassVar[java.lang.Class]


class OutputsTableModel(VarTableModel[OutputColumn, OutputRow]):

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, tool: ghidra.framework.plugintool.ServiceProvider, dialog: DebuggerEmulateFunctionDialog) -> None:
        ...


@typing.type_check_only
class OutputColumn(java.lang.Enum[OutputColumn], docking.widgets.table.EnumeratedTableColumn[OutputColumn, OutputRow]):

    class_: typing.ClassVar[java.lang.Class]
    NAME: typing.Final[OutputColumn]
    STORAGE: typing.Final[OutputColumn]
    VALUE: typing.Final[OutputColumn]
    TYPE: typing.Final[OutputColumn]
    REPR: typing.Final[OutputColumn]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OutputColumn:
        ...

    @staticmethod
    def values() -> jpype.JArray[OutputColumn]:
        ...


@typing.type_check_only
class OutputRow(VarRow):
    ...
    class_: typing.ClassVar[java.lang.Class]


@typing.type_check_only
class VarStorage(java.lang.Record):

    class_: typing.ClassVar[java.lang.Class]

    @typing.overload
    def deref(self, language: ghidra.app.plugin.processors.sleigh.SleighLanguage, space: ghidra.program.model.address.AddressSpace, length: typing.Union[jpype.JInt, int]) -> VarStorage:
        ...

    @typing.overload
    def deref(self, language: ghidra.app.plugin.processors.sleigh.SleighLanguage, space: ghidra.program.model.address.AddressSpace, offset: typing.Union[jpype.JInt, int], length: typing.Union[jpype.JInt, int]) -> VarStorage:
        ...

    def equals(self, o: java.lang.Object) -> bool:
        ...

    def hashCode(self) -> int:
        ...

    def nodes(self) -> java.util.List[VarStorageNode]:
        ...

    def size(self) -> int:
        ...


@typing.type_check_only
class VarStorageNode(java.lang.Record):

    class_: typing.ClassVar[java.lang.Class]

    def address(self) -> ghidra.program.model.address.Address:
        ...

    @typing.overload
    def deref(self, language: ghidra.app.plugin.processors.sleigh.SleighLanguage, space: ghidra.program.model.address.AddressSpace, length: typing.Union[jpype.JInt, int]) -> VarStorageNode:
        ...

    @typing.overload
    def deref(self, language: ghidra.app.plugin.processors.sleigh.SleighLanguage, space: ghidra.program.model.address.AddressSpace, offset: typing.Union[jpype.JInt, int], length: typing.Union[jpype.JInt, int]) -> VarStorageNode:
        ...

    def equals(self, o: java.lang.Object) -> bool:
        ...

    def expr(self) -> str:
        ...

    def hashCode(self) -> int:
        ...

    def size(self) -> int:
        ...


@typing.type_check_only
class RawStyle(java.lang.Enum[RawStyle]):

    class_: typing.ClassVar[java.lang.Class]
    INTEGER: typing.Final[RawStyle]
    BYTES: typing.Final[RawStyle]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> RawStyle:
        ...

    @staticmethod
    def values() -> jpype.JArray[RawStyle]:
        ...


class LongInputVerifier(javax.swing.InputVerifier):

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self) -> None:
        ...


@typing.type_check_only
class VarDataTypeEditor(ghidra.base.widgets.table.AbstractDataTypeTableCellEditor):
    ...
    class_: typing.ClassVar[java.lang.Class]



__all__ = ["AddressInputVerifier", "LocAndVal", "FunctionEmulationHarness", "DebuggerEmulateFunctionDialog", "InputsTableModel", "VarTableModel", "InputColumn", "InputRow", "VarRow", "OutputsTableModel", "OutputColumn", "OutputRow", "VarStorage", "VarStorageNode", "RawStyle", "LongInputVerifier", "VarDataTypeEditor"]
