using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Globalization;
using System.Net;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace Qint.Wrapper
{

internal enum NewEventPropertyId : ulong
{
	ConId,
	Content,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_new_event_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_new_event(IntPtr self);
}

public partial class NewEvent : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public ConnectionId? ConId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) NewEventPropertyId.ConId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return new ConnectionId { Value = val };
		}
	}
	public EventContent Content
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) NewEventPropertyId.Content);
			return new EventContent(basePtr, baseFunction, id);
		}
	}

	internal NewEvent(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~NewEvent()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum EventContentPropertyId : ulong
{
	Type,
	FutureFinishedHandle,
	FutureFinishedError,
	LibEvent,
}

public enum EventContentType : ulong
{
	ConnectionAdded,
	ConnectionRemoved,
	FutureFinished,
	LibEvent,
}


internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_event_content_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_event_content(IntPtr self);
}

public partial class EventContent : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public EventContentType Type
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) EventContentPropertyId.Type;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
			baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			return (EventContentType) result.content;
		}
	}
	public ulong FutureFinishedHandle
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) EventContentPropertyId.FutureFinishedHandle;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public string FutureFinishedError
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) EventContentPropertyId.FutureFinishedError;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public Event LibEvent
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) EventContentPropertyId.LibEvent);
			return new Event(basePtr, baseFunction, id);
		}
	}

	internal EventContent(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~EventContent()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum NewFfiInvokerPropertyId : ulong
{
	Name,
	Uid,
	Id,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_new_ffi_invoker_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_new_ffi_invoker(IntPtr self);
}

public partial class NewFfiInvoker : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public string Name
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) NewFfiInvokerPropertyId.Name;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public string Uid
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) NewFfiInvokerPropertyId.Uid;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public ushort Id
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) NewFfiInvokerPropertyId.Id;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ushort) val;
		}
	}

	internal NewFfiInvoker(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~NewFfiInvoker()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum EventPropertyId : ulong
{
	Type,
	PropertyAddedId,
	PropertyAddedInvoker,
	PropertyChangedId,
	PropertyChangedOld,
	PropertyChangedInvoker,
	PropertyRemovedId,
	PropertyRemovedOld,
	PropertyRemovedInvoker,
	MessageFrom,
	MessageInvoker,
	MessageMessage,
}

public enum EventType : ulong
{
	PropertyAdded,
	PropertyChanged,
	PropertyRemoved,
	Message,
	__NonExhaustive,
}


internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_event_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_event(IntPtr self);
}

public partial class Event : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public EventType Type
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) EventPropertyId.Type;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
			baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			return (EventType) result.content;
		}
	}
	public PropertyId PropertyAddedId
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) EventPropertyId.PropertyAddedId);
			return new PropertyId(basePtr, baseFunction, id);
		}
	}
	public Invoker PropertyAddedInvoker
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) EventPropertyId.PropertyAddedInvoker);
			return new Invoker(basePtr, baseFunction, id);
		}
	}
	public PropertyId PropertyChangedId
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) EventPropertyId.PropertyChangedId);
			return new PropertyId(basePtr, baseFunction, id);
		}
	}
	public PropertyValue PropertyChangedOld
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) EventPropertyId.PropertyChangedOld);
			return new PropertyValue(basePtr, baseFunction, id);
		}
	}
	public Invoker PropertyChangedInvoker
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) EventPropertyId.PropertyChangedInvoker);
			return new Invoker(basePtr, baseFunction, id);
		}
	}
	public PropertyId PropertyRemovedId
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) EventPropertyId.PropertyRemovedId);
			return new PropertyId(basePtr, baseFunction, id);
		}
	}
	public PropertyValue PropertyRemovedOld
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) EventPropertyId.PropertyRemovedOld);
			return new PropertyValue(basePtr, baseFunction, id);
		}
	}
	public Invoker PropertyRemovedInvoker
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) EventPropertyId.PropertyRemovedInvoker);
			return new Invoker(basePtr, baseFunction, id);
		}
	}
	public MessageTarget MessageFrom
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) EventPropertyId.MessageFrom);
			return new MessageTarget(basePtr, baseFunction, id);
		}
	}
	public Invoker MessageInvoker
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) EventPropertyId.MessageInvoker);
			return new Invoker(basePtr, baseFunction, id);
		}
	}
	public string MessageMessage
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) EventPropertyId.MessageMessage;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}

	internal Event(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~Event()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum MessageTargetPropertyId : ulong
{
	Type,
	Client,
	Poke,
}

public enum MessageTargetType : ulong
{
	Server,
	Channel,
	Client,
	Poke,
}


internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_message_target_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_message_target(IntPtr self);
}

public partial class MessageTarget : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public MessageTargetType Type
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) MessageTargetPropertyId.Type;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
			baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			return (MessageTargetType) result.content;
		}
	}
	public ClientId Client
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) MessageTargetPropertyId.Client;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId Poke
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) MessageTargetPropertyId.Poke;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}

	internal MessageTarget(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~MessageTarget()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum MaxClientsPropertyId : ulong
{
	Type,
	Limited,
}

public enum MaxClientsType : ulong
{
	Unlimited,
	Inherited,
	Limited,
}


internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_max_clients_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_max_clients(IntPtr self);
}

public partial class MaxClients : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public MaxClientsType Type
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) MaxClientsPropertyId.Type;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
			baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			return (MaxClientsType) result.content;
		}
	}
	public ushort Limited
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) MaxClientsPropertyId.Limited;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ushort) val;
		}
	}

	internal MaxClients(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~MaxClients()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum TalkPowerRequestPropertyId : ulong
{
	Time,
	Message,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_talk_power_request_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_talk_power_request(IntPtr self);
}

public partial class TalkPowerRequest : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public DateTimeOffset Time
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) TalkPowerRequestPropertyId.Time;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return DateTimeOffset.FromUnixTimeSeconds((long) val);
		}
	}
	public string Message
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) TalkPowerRequestPropertyId.Message;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}

	internal TalkPowerRequest(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~TalkPowerRequest()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum InvokerPropertyId : ulong
{
	Name,
	Id,
	Uid,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_invoker_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_invoker(IntPtr self);
}

public partial class Invoker : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public string Name
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) InvokerPropertyId.Name;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public ClientId Id
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) InvokerPropertyId.Id;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public Uid Uid
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) InvokerPropertyId.Uid;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return new Uid { Value = NativeMethods.StringFromNativeUtf8((IntPtr) val) };
		}
	}

	internal Invoker(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~Invoker()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum PropertyIdPropertyId : ulong
{
	Type,
	ServerGroup,
	File0,
	File1,
	OptionalChannelData,
	Channel,
	OptionalClientData,
	ConnectionClientData,
	Client,
	ChatEntry,
	ServerGroupId,
	ServerGroupName,
	ServerGroupGroupType,
	ServerGroupIconId,
	ServerGroupIsPermanent,
	ServerGroupSortId,
	ServerGroupNamingMode,
	ServerGroupNeededModifyPower,
	ServerGroupNeededMemberAddPower,
	ServerGroupNeededMemberRemovePower,
	FilePath0,
	FilePath1,
	FileSize0,
	FileSize1,
	FileLastChanged0,
	FileLastChanged1,
	FileIsFile0,
	FileIsFile1,
	OptionalChannelDataDescription,
	ChannelId,
	ChannelParent,
	ChannelName,
	ChannelTopic,
	ChannelCodec,
	ChannelCodecQuality,
	ChannelMaxClients,
	ChannelMaxFamilyClients,
	ChannelOrder,
	ChannelChannelType,
	ChannelIsDefault,
	ChannelHasPassword,
	ChannelCodecLatencyFactor,
	ChannelIsUnencrypted,
	ChannelDeleteDelay,
	ChannelNeededTalkPower,
	ChannelForcedSilence,
	ChannelPhoneticName,
	ChannelIconId,
	ChannelIsPrivate,
	ChannelSubscribed,
	OptionalClientDataVersion,
	OptionalClientDataPlatform,
	OptionalClientDataLoginName,
	OptionalClientDataCreated,
	OptionalClientDataLastConnected,
	OptionalClientDataTotalConnection,
	OptionalClientDataMonthBytesUploaded,
	OptionalClientDataMonthBytesDownloaded,
	OptionalClientDataTotalBytesUploaded,
	OptionalClientDataTotalBytesDownloaded,
	ConnectionClientDataPing,
	ConnectionClientDataPingDeviation,
	ConnectionClientDataConnectedTime,
	ConnectionClientDataClientAddress,
	ConnectionClientDataPacketsSentSpeech,
	ConnectionClientDataPacketsSentKeepalive,
	ConnectionClientDataPacketsSentControl,
	ConnectionClientDataBytesSentSpeech,
	ConnectionClientDataBytesSentKeepalive,
	ConnectionClientDataBytesSentControl,
	ConnectionClientDataPacketsReceivedSpeech,
	ConnectionClientDataPacketsReceivedKeepalive,
	ConnectionClientDataPacketsReceivedControl,
	ConnectionClientDataBytesReceivedSpeech,
	ConnectionClientDataBytesReceivedKeepalive,
	ConnectionClientDataBytesReceivedControl,
	ConnectionClientDataServerToClientPacketlossSpeech,
	ConnectionClientDataServerToClientPacketlossKeepalive,
	ConnectionClientDataServerToClientPacketlossControl,
	ConnectionClientDataServerToClientPacketlossTotal,
	ConnectionClientDataClientToServerPacketlossSpeech,
	ConnectionClientDataClientToServerPacketlossKeepalive,
	ConnectionClientDataClientToServerPacketlossControl,
	ConnectionClientDataClientToServerPacketlossTotal,
	ConnectionClientDataBandwidthSentLastSecondSpeech,
	ConnectionClientDataBandwidthSentLastSecondKeepalive,
	ConnectionClientDataBandwidthSentLastSecondControl,
	ConnectionClientDataBandwidthSentLastMinuteSpeech,
	ConnectionClientDataBandwidthSentLastMinuteKeepalive,
	ConnectionClientDataBandwidthSentLastMinuteControl,
	ConnectionClientDataBandwidthReceivedLastSecondSpeech,
	ConnectionClientDataBandwidthReceivedLastSecondKeepalive,
	ConnectionClientDataBandwidthReceivedLastSecondControl,
	ConnectionClientDataBandwidthReceivedLastMinuteSpeech,
	ConnectionClientDataBandwidthReceivedLastMinuteKeepalive,
	ConnectionClientDataBandwidthReceivedLastMinuteControl,
	ConnectionClientDataFiletransferBandwidthSent,
	ConnectionClientDataFiletransferBandwidthReceived,
	ConnectionClientDataIdleTime,
	ClientId,
	ClientChannel,
	ClientUid,
	ClientName,
	ClientInputMuted,
	ClientOutputMuted,
	ClientOutputOnlyMuted,
	ClientInputHardwareEnabled,
	ClientOutputHardwareEnabled,
	ClientTalkPowerGranted,
	ClientMetadata,
	ClientIsRecording,
	ClientDatabaseId,
	ClientChannelGroup,
	ClientServerGroup0,
	ClientServerGroup1,
	ClientAwayMessage,
	ClientClientType,
	ClientAvatarHash,
	ClientTalkPower,
	ClientTalkPowerRequest,
	ClientDescription,
	ClientIsPrioritySpeaker,
	ClientUnreadMessages,
	ClientPhoneticName,
	ClientNeededServerqueryViewPower,
	ClientIconId,
	ClientIsChannelCommander,
	ClientCountryCode,
	ClientInheritedChannelGroupFromChannel,
	ClientBadges,
	ServerIp,
	ChatEntrySenderClient,
	ChatEntryText,
	ChatEntryDate,
	ChatEntryMode,
}

public enum PropertyIdType : ulong
{
	ServerGroup,
	File,
	OptionalChannelData,
	Channel,
	OptionalClientData,
	ConnectionClientData,
	Client,
	OptionalServerData,
	ConnectionServerData,
	Server,
	Connection,
	ChatEntry,
	ServerGroupId,
	ServerGroupName,
	ServerGroupGroupType,
	ServerGroupIconId,
	ServerGroupIsPermanent,
	ServerGroupSortId,
	ServerGroupNamingMode,
	ServerGroupNeededModifyPower,
	ServerGroupNeededMemberAddPower,
	ServerGroupNeededMemberRemovePower,
	FilePath,
	FileSize,
	FileLastChanged,
	FileIsFile,
	OptionalChannelDataDescription,
	ChannelId,
	ChannelParent,
	ChannelName,
	ChannelTopic,
	ChannelCodec,
	ChannelCodecQuality,
	ChannelMaxClients,
	ChannelMaxFamilyClients,
	ChannelOrder,
	ChannelChannelType,
	ChannelIsDefault,
	ChannelHasPassword,
	ChannelCodecLatencyFactor,
	ChannelIsUnencrypted,
	ChannelDeleteDelay,
	ChannelNeededTalkPower,
	ChannelForcedSilence,
	ChannelPhoneticName,
	ChannelIconId,
	ChannelIsPrivate,
	ChannelSubscribed,
	OptionalClientDataVersion,
	OptionalClientDataPlatform,
	OptionalClientDataLoginName,
	OptionalClientDataCreated,
	OptionalClientDataLastConnected,
	OptionalClientDataTotalConnection,
	OptionalClientDataMonthBytesUploaded,
	OptionalClientDataMonthBytesDownloaded,
	OptionalClientDataTotalBytesUploaded,
	OptionalClientDataTotalBytesDownloaded,
	ConnectionClientDataPing,
	ConnectionClientDataPingDeviation,
	ConnectionClientDataConnectedTime,
	ConnectionClientDataClientAddress,
	ConnectionClientDataPacketsSentSpeech,
	ConnectionClientDataPacketsSentKeepalive,
	ConnectionClientDataPacketsSentControl,
	ConnectionClientDataBytesSentSpeech,
	ConnectionClientDataBytesSentKeepalive,
	ConnectionClientDataBytesSentControl,
	ConnectionClientDataPacketsReceivedSpeech,
	ConnectionClientDataPacketsReceivedKeepalive,
	ConnectionClientDataPacketsReceivedControl,
	ConnectionClientDataBytesReceivedSpeech,
	ConnectionClientDataBytesReceivedKeepalive,
	ConnectionClientDataBytesReceivedControl,
	ConnectionClientDataServerToClientPacketlossSpeech,
	ConnectionClientDataServerToClientPacketlossKeepalive,
	ConnectionClientDataServerToClientPacketlossControl,
	ConnectionClientDataServerToClientPacketlossTotal,
	ConnectionClientDataClientToServerPacketlossSpeech,
	ConnectionClientDataClientToServerPacketlossKeepalive,
	ConnectionClientDataClientToServerPacketlossControl,
	ConnectionClientDataClientToServerPacketlossTotal,
	ConnectionClientDataBandwidthSentLastSecondSpeech,
	ConnectionClientDataBandwidthSentLastSecondKeepalive,
	ConnectionClientDataBandwidthSentLastSecondControl,
	ConnectionClientDataBandwidthSentLastMinuteSpeech,
	ConnectionClientDataBandwidthSentLastMinuteKeepalive,
	ConnectionClientDataBandwidthSentLastMinuteControl,
	ConnectionClientDataBandwidthReceivedLastSecondSpeech,
	ConnectionClientDataBandwidthReceivedLastSecondKeepalive,
	ConnectionClientDataBandwidthReceivedLastSecondControl,
	ConnectionClientDataBandwidthReceivedLastMinuteSpeech,
	ConnectionClientDataBandwidthReceivedLastMinuteKeepalive,
	ConnectionClientDataBandwidthReceivedLastMinuteControl,
	ConnectionClientDataFiletransferBandwidthSent,
	ConnectionClientDataFiletransferBandwidthReceived,
	ConnectionClientDataIdleTime,
	ClientId,
	ClientChannel,
	ClientUid,
	ClientName,
	ClientInputMuted,
	ClientOutputMuted,
	ClientOutputOnlyMuted,
	ClientInputHardwareEnabled,
	ClientOutputHardwareEnabled,
	ClientTalkPowerGranted,
	ClientMetadata,
	ClientIsRecording,
	ClientDatabaseId,
	ClientChannelGroup,
	ClientServerGroup,
	ClientAwayMessage,
	ClientClientType,
	ClientAvatarHash,
	ClientTalkPower,
	ClientTalkPowerRequest,
	ClientDescription,
	ClientIsPrioritySpeaker,
	ClientUnreadMessages,
	ClientPhoneticName,
	ClientNeededServerqueryViewPower,
	ClientIconId,
	ClientIsChannelCommander,
	ClientCountryCode,
	ClientInheritedChannelGroupFromChannel,
	ClientBadges,
	OptionalServerDataConnectionCount,
	OptionalServerDataChannelCount,
	OptionalServerDataUptime,
	OptionalServerDataHasPassword,
	OptionalServerDataDefaultChannelAdminGroup,
	OptionalServerDataMaxDownloadTotalBandwith,
	OptionalServerDataMaxUploadTotalBandwith,
	OptionalServerDataComplainAutobanCount,
	OptionalServerDataComplainAutobanTime,
	OptionalServerDataComplainRemoveTime,
	OptionalServerDataMinClientsForceSilence,
	OptionalServerDataAntifloodPointsTickReduce,
	OptionalServerDataAntifloodPointsNeededCommandBlock,
	OptionalServerDataClientCount,
	OptionalServerDataQueryCount,
	OptionalServerDataQueryOnlineCount,
	OptionalServerDataDownloadQuota,
	OptionalServerDataUploadQuota,
	OptionalServerDataMonthBytesDownloaded,
	OptionalServerDataMonthBytesUploaded,
	OptionalServerDataTotalBytesDownloaded,
	OptionalServerDataTotalBytesUploaded,
	OptionalServerDataPort,
	OptionalServerDataAutostart,
	OptionalServerDataMachineId,
	OptionalServerDataNeededIdentitySecurityLevel,
	OptionalServerDataLogClient,
	OptionalServerDataLogQuery,
	OptionalServerDataLogChannel,
	OptionalServerDataLogPermissions,
	OptionalServerDataLogServer,
	OptionalServerDataLogFileTransfer,
	OptionalServerDataMinClientVersion,
	OptionalServerDataReservedSlots,
	OptionalServerDataTotalPacketlossSpeech,
	OptionalServerDataTotalPacketlossKeepalive,
	OptionalServerDataTotalPacketlossControl,
	OptionalServerDataTotalPacketlossTotal,
	OptionalServerDataTotalPing,
	OptionalServerDataWeblistEnabled,
	OptionalServerDataMinAndroidVersion,
	OptionalServerDataMinIosVersion,
	ConnectionServerDataFileTransferBandwidthSent,
	ConnectionServerDataFileTransferBandwidthReceived,
	ConnectionServerDataFileTransferBytesSentTotal,
	ConnectionServerDataFileTransferBytesReceivedTotal,
	ConnectionServerDataPacketsSentTotal,
	ConnectionServerDataBytesSentTotal,
	ConnectionServerDataPacketsReceivedTotal,
	ConnectionServerDataBytesReceivedTotal,
	ConnectionServerDataBandwidthSentLastSecondTotal,
	ConnectionServerDataBandwidthSentLastMinuteTotal,
	ConnectionServerDataBandwidthReceivedLastSecondTotal,
	ConnectionServerDataBandwidthReceivedLastMinuteTotal,
	ConnectionServerDataConnectedTime,
	ConnectionServerDataPacketlossTotal,
	ConnectionServerDataPing,
	ServerUid,
	ServerVirtualServerId,
	ServerName,
	ServerWelcomeMessage,
	ServerPlatform,
	ServerVersion,
	ServerMaxClients,
	ServerCreated,
	ServerCodecEncryptionMode,
	ServerHostmessage,
	ServerHostmessageMode,
	ServerDefaultServerGroup,
	ServerDefaultChannelGroup,
	ServerHostbannerUrl,
	ServerHostbannerGfxUrl,
	ServerHostbannerGfxInterval,
	ServerPrioritySpeakerDimmModificator,
	ServerHostbuttonTooltip,
	ServerHostbuttonUrl,
	ServerHostbuttonGfxUrl,
	ServerPhoneticName,
	ServerIconId,
	ServerIp,
	ServerAskForPrivilegekey,
	ServerHostbannerMode,
	ServerTempChannelDefaultDeleteDelay,
	ServerProtocolVersion,
	ServerLicense,
	ConnectionOwnClient,
	ChatEntrySenderClient,
	ChatEntryText,
	ChatEntryDate,
	ChatEntryMode,
	__NonExhaustive,
}


internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_property_id_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_property_id(IntPtr self);
}

public partial class PropertyId : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public PropertyIdType Type
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.Type;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
			baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			return (PropertyIdType) result.content;
		}
	}
	public ServerGroupId ServerGroup
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ServerGroup;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public ChannelId File0
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.File0;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public string File1
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.File1;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public ChannelId OptionalChannelData
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.OptionalChannelData;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId Channel
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.Channel;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ClientId OptionalClientData
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.OptionalClientData;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientData
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientData;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId Client
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.Client;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ChatEntry
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChatEntry;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ServerGroupId ServerGroupId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ServerGroupId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public ServerGroupId ServerGroupName
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ServerGroupName;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public ServerGroupId ServerGroupGroupType
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ServerGroupGroupType;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public ServerGroupId ServerGroupIconId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ServerGroupIconId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public ServerGroupId ServerGroupIsPermanent
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ServerGroupIsPermanent;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public ServerGroupId ServerGroupSortId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ServerGroupSortId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public ServerGroupId ServerGroupNamingMode
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ServerGroupNamingMode;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public ServerGroupId ServerGroupNeededModifyPower
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ServerGroupNeededModifyPower;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public ServerGroupId ServerGroupNeededMemberAddPower
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ServerGroupNeededMemberAddPower;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public ServerGroupId ServerGroupNeededMemberRemovePower
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ServerGroupNeededMemberRemovePower;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public ChannelId FilePath0
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.FilePath0;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public string FilePath1
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.FilePath1;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public ChannelId FileSize0
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.FileSize0;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public string FileSize1
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.FileSize1;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public ChannelId FileLastChanged0
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.FileLastChanged0;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public string FileLastChanged1
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.FileLastChanged1;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public ChannelId FileIsFile0
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.FileIsFile0;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public string FileIsFile1
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.FileIsFile1;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public ChannelId OptionalChannelDataDescription
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.OptionalChannelDataDescription;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelParent
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelParent;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelName
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelName;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelTopic
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelTopic;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelCodec
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelCodec;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelCodecQuality
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelCodecQuality;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelMaxClients
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelMaxClients;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelMaxFamilyClients
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelMaxFamilyClients;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelOrder
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelOrder;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelChannelType
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelChannelType;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelIsDefault
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelIsDefault;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelHasPassword
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelHasPassword;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelCodecLatencyFactor
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelCodecLatencyFactor;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelIsUnencrypted
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelIsUnencrypted;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelDeleteDelay
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelDeleteDelay;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelNeededTalkPower
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelNeededTalkPower;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelForcedSilence
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelForcedSilence;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelPhoneticName
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelPhoneticName;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelIconId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelIconId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelIsPrivate
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelIsPrivate;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId ChannelSubscribed
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChannelSubscribed;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ClientId OptionalClientDataVersion
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.OptionalClientDataVersion;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId OptionalClientDataPlatform
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.OptionalClientDataPlatform;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId OptionalClientDataLoginName
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.OptionalClientDataLoginName;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId OptionalClientDataCreated
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.OptionalClientDataCreated;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId OptionalClientDataLastConnected
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.OptionalClientDataLastConnected;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId OptionalClientDataTotalConnection
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.OptionalClientDataTotalConnection;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId OptionalClientDataMonthBytesUploaded
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.OptionalClientDataMonthBytesUploaded;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId OptionalClientDataMonthBytesDownloaded
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.OptionalClientDataMonthBytesDownloaded;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId OptionalClientDataTotalBytesUploaded
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.OptionalClientDataTotalBytesUploaded;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId OptionalClientDataTotalBytesDownloaded
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.OptionalClientDataTotalBytesDownloaded;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataPing
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataPing;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataPingDeviation
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataPingDeviation;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataConnectedTime
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataConnectedTime;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataClientAddress
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataClientAddress;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataPacketsSentSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataPacketsSentSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataPacketsSentKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataPacketsSentKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataPacketsSentControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataPacketsSentControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBytesSentSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBytesSentSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBytesSentKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBytesSentKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBytesSentControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBytesSentControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataPacketsReceivedSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataPacketsReceivedSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataPacketsReceivedKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataPacketsReceivedKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataPacketsReceivedControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataPacketsReceivedControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBytesReceivedSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBytesReceivedSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBytesReceivedKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBytesReceivedKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBytesReceivedControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBytesReceivedControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataServerToClientPacketlossSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataServerToClientPacketlossSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataServerToClientPacketlossKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataServerToClientPacketlossKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataServerToClientPacketlossControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataServerToClientPacketlossControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataServerToClientPacketlossTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataServerToClientPacketlossTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataClientToServerPacketlossSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataClientToServerPacketlossSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataClientToServerPacketlossKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataClientToServerPacketlossKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataClientToServerPacketlossControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataClientToServerPacketlossControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataClientToServerPacketlossTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataClientToServerPacketlossTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBandwidthSentLastSecondSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBandwidthSentLastSecondSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBandwidthSentLastSecondKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBandwidthSentLastSecondKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBandwidthSentLastSecondControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBandwidthSentLastSecondControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBandwidthSentLastMinuteSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBandwidthSentLastMinuteSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBandwidthSentLastMinuteKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBandwidthSentLastMinuteKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBandwidthSentLastMinuteControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBandwidthSentLastMinuteControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBandwidthReceivedLastSecondSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBandwidthReceivedLastSecondSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBandwidthReceivedLastSecondKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBandwidthReceivedLastSecondKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBandwidthReceivedLastSecondControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBandwidthReceivedLastSecondControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBandwidthReceivedLastMinuteSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBandwidthReceivedLastMinuteSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBandwidthReceivedLastMinuteKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBandwidthReceivedLastMinuteKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataBandwidthReceivedLastMinuteControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataBandwidthReceivedLastMinuteControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataFiletransferBandwidthSent
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataFiletransferBandwidthSent;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataFiletransferBandwidthReceived
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataFiletransferBandwidthReceived;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ConnectionClientDataIdleTime
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ConnectionClientDataIdleTime;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientChannel
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientChannel;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientUid
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientUid;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientName
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientName;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientInputMuted
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientInputMuted;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientOutputMuted
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientOutputMuted;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientOutputOnlyMuted
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientOutputOnlyMuted;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientInputHardwareEnabled
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientInputHardwareEnabled;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientOutputHardwareEnabled
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientOutputHardwareEnabled;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientTalkPowerGranted
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientTalkPowerGranted;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientMetadata
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientMetadata;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientIsRecording
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientIsRecording;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientDatabaseId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientDatabaseId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientChannelGroup
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientChannelGroup;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientServerGroup0
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientServerGroup0;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ServerGroupId ClientServerGroup1
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientServerGroup1;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public ClientId ClientAwayMessage
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientAwayMessage;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientClientType
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientClientType;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientAvatarHash
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientAvatarHash;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientTalkPower
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientTalkPower;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientTalkPowerRequest
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientTalkPowerRequest;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientDescription
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientDescription;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientIsPrioritySpeaker
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientIsPrioritySpeaker;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientUnreadMessages
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientUnreadMessages;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientPhoneticName
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientPhoneticName;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientNeededServerqueryViewPower
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientNeededServerqueryViewPower;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientIconId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientIconId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientIsChannelCommander
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientIsChannelCommander;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientCountryCode
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientCountryCode;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientInheritedChannelGroupFromChannel
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientInheritedChannelGroupFromChannel;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ClientBadges
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ClientBadges;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public string ServerIp
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ServerIp;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public ClientId ChatEntrySenderClient
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChatEntrySenderClient;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ChatEntryText
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChatEntryText;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ChatEntryDate
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChatEntryDate;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ClientId ChatEntryMode
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyIdPropertyId.ChatEntryMode;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}

	internal PropertyId(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~PropertyId()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum PropertyValuePropertyId : ulong
{
	Type,
	ServerGroup,
	File,
	OptionalChannelData,
	Channel,
	OptionalClientData,
	ConnectionClientData,
	Client,
	OptionalServerData,
	ConnectionServerData,
	Server,
	Connection,
	ChatEntry,
	ServerGroupId,
	String,
	GroupType,
	IconHash,
	Bool,
	I32,
	GroupNamingMode,
	OptionI32,
	I64,
	DateTimeUtc,
	ChannelId,
	OptionString,
	OptionCodec,
	OptionU8,
	OptionMaxClients,
	ChannelType,
	OptionBool,
	OptionDuration,
	OptionIconHash,
	U32,
	U64,
	Duration,
	OptionSocketAddr,
	F32,
	ClientId,
	Uid,
	ClientDbId,
	ChannelGroupId,
	ClientType,
	OptionTalkPowerRequest,
	U16,
	U8,
	CodecEncryptionMode,
	HostMessageMode,
	HostBannerMode,
	LicenseType,
	TextMessageTargetMode,
}

public enum PropertyValueType : ulong
{
	ServerGroup,
	File,
	OptionalChannelData,
	Channel,
	OptionalClientData,
	ConnectionClientData,
	Client,
	OptionalServerData,
	ConnectionServerData,
	Server,
	Connection,
	ChatEntry,
	ServerGroupId,
	String,
	GroupType,
	IconHash,
	Bool,
	I32,
	GroupNamingMode,
	OptionI32,
	I64,
	DateTimeUtc,
	ChannelId,
	OptionString,
	OptionCodec,
	OptionU8,
	OptionMaxClients,
	ChannelType,
	OptionBool,
	OptionDuration,
	OptionIconHash,
	U32,
	U64,
	Duration,
	OptionSocketAddr,
	F32,
	ClientId,
	Uid,
	ClientDbId,
	ChannelGroupId,
	ClientType,
	OptionTalkPowerRequest,
	U16,
	U8,
	CodecEncryptionMode,
	HostMessageMode,
	HostBannerMode,
	LicenseType,
	TextMessageTargetMode,
	__NonExhaustive,
}


internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_property_value_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_property_value(IntPtr self);
}

public partial class PropertyValue : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public PropertyValueType Type
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.Type;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
			baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			return (PropertyValueType) result.content;
		}
	}
	public ServerGroup ServerGroup
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) PropertyValuePropertyId.ServerGroup);
			return new ServerGroup(basePtr, baseFunction, id);
		}
	}
	public File File
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) PropertyValuePropertyId.File);
			return new File(basePtr, baseFunction, id);
		}
	}
	public OptionalChannelData OptionalChannelData
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) PropertyValuePropertyId.OptionalChannelData);
			return new OptionalChannelData(basePtr, baseFunction, id);
		}
	}
	public Channel Channel
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) PropertyValuePropertyId.Channel);
			return new Channel(basePtr, baseFunction, id);
		}
	}
	public OptionalClientData OptionalClientData
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) PropertyValuePropertyId.OptionalClientData);
			return new OptionalClientData(basePtr, baseFunction, id);
		}
	}
	public ConnectionClientData ConnectionClientData
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) PropertyValuePropertyId.ConnectionClientData);
			return new ConnectionClientData(basePtr, baseFunction, id);
		}
	}
	public Client Client
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) PropertyValuePropertyId.Client);
			return new Client(basePtr, baseFunction, id);
		}
	}
	public OptionalServerData OptionalServerData
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) PropertyValuePropertyId.OptionalServerData);
			return new OptionalServerData(basePtr, baseFunction, id);
		}
	}
	public ConnectionServerData ConnectionServerData
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) PropertyValuePropertyId.ConnectionServerData);
			return new ConnectionServerData(basePtr, baseFunction, id);
		}
	}
	public Server Server
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) PropertyValuePropertyId.Server);
			return new Server(basePtr, baseFunction, id);
		}
	}
	public Connection Connection
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) PropertyValuePropertyId.Connection);
			return new Connection(basePtr, baseFunction, id);
		}
	}
	public ChatEntry ChatEntry
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) PropertyValuePropertyId.ChatEntry);
			return new ChatEntry(basePtr, baseFunction, id);
		}
	}
	public ServerGroupId ServerGroupId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.ServerGroupId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public string String
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.String;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public GroupType GroupType
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.GroupType;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (GroupType) val;
		}
	}
	public IconHash IconHash
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.IconHash;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new IconHash { Value = val };
		}
	}
	public bool Bool
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.Bool;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public int I32
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.I32;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (int) val;
		}
	}
	public GroupNamingMode GroupNamingMode
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.GroupNamingMode;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (GroupNamingMode) val;
		}
	}
	public int? OptionI32
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.OptionI32;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return (int) val;
		}
	}
	public long I64
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.I64;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (long) val;
		}
	}
	public DateTimeOffset DateTimeUtc
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.DateTimeUtc;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return DateTimeOffset.FromUnixTimeSeconds((long) val);
		}
	}
	public ChannelId ChannelId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.ChannelId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public string OptionString
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.OptionString;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public Codec? OptionCodec
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.OptionCodec;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return (Codec) val;
		}
	}
	public byte? OptionU8
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.OptionU8;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return (byte) val;
		}
	}
	public MaxClients OptionMaxClients
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) PropertyValuePropertyId.OptionMaxClients);
			return new MaxClients(basePtr, baseFunction, id);
		}
	}
	public ChannelType ChannelType
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.ChannelType;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ChannelType) val;
		}
	}
	public bool? OptionBool
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.OptionBool;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return val != 0;
		}
	}
	public TimeSpan? OptionDuration
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.OptionDuration;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}
	public IconHash? OptionIconHash
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.OptionIconHash;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return new IconHash { Value = val };
		}
	}
	public uint U32
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.U32;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (uint) val;
		}
	}
	public ulong U64
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.U64;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public TimeSpan Duration
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.Duration;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}
	public IPEndPoint OptionSocketAddr
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.OptionSocketAddr;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return NativeMethods.ParseIPEndPoint(NativeMethods.StringFromNativeUtf8((IntPtr) val));
		}
	}
	public float F32
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.F32;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public ClientId ClientId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.ClientId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public Uid Uid
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.Uid;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new Uid { Value = NativeMethods.StringFromNativeUtf8((IntPtr) val) };
		}
	}
	public ClientDbId ClientDbId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.ClientDbId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientDbId { Value = val };
		}
	}
	public ChannelGroupId ChannelGroupId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.ChannelGroupId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelGroupId { Value = val };
		}
	}
	public ClientType ClientType
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.ClientType;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ClientType) val;
		}
	}
	public TalkPowerRequest OptionTalkPowerRequest
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) PropertyValuePropertyId.OptionTalkPowerRequest);
			return new TalkPowerRequest(basePtr, baseFunction, id);
		}
	}
	public ushort U16
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.U16;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ushort) val;
		}
	}
	public byte U8
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.U8;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (byte) val;
		}
	}
	public CodecEncryptionMode CodecEncryptionMode
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.CodecEncryptionMode;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (CodecEncryptionMode) val;
		}
	}
	public HostMessageMode HostMessageMode
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.HostMessageMode;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (HostMessageMode) val;
		}
	}
	public HostBannerMode HostBannerMode
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.HostBannerMode;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (HostBannerMode) val;
		}
	}
	public LicenseType LicenseType
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.LicenseType;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (LicenseType) val;
		}
	}
	public TextMessageTargetMode TextMessageTargetMode
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) PropertyValuePropertyId.TextMessageTargetMode;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (TextMessageTargetMode) val;
		}
	}

	internal PropertyValue(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~PropertyValue()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum ServerGroupPropertyId : ulong
{
	Id,
	Name,
	GroupType,
	IconId,
	IsPermanent,
	SortId,
	NamingMode,
	NeededModifyPower,
	NeededMemberAddPower,
	NeededMemberRemovePower,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_server_group_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_server_group(IntPtr self);
}

public partial class ServerGroup : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public ServerGroupId Id
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerGroupPropertyId.Id;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public string Name
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerGroupPropertyId.Name;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public GroupType GroupType
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerGroupPropertyId.GroupType;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (GroupType) val;
		}
	}
	public IconHash IconId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerGroupPropertyId.IconId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new IconHash { Value = val };
		}
	}
	public bool IsPermanent
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerGroupPropertyId.IsPermanent;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public int SortId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerGroupPropertyId.SortId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (int) val;
		}
	}
	public GroupNamingMode NamingMode
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerGroupPropertyId.NamingMode;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (GroupNamingMode) val;
		}
	}
	public int NeededModifyPower
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerGroupPropertyId.NeededModifyPower;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (int) val;
		}
	}
	public int NeededMemberAddPower
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerGroupPropertyId.NeededMemberAddPower;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (int) val;
		}
	}
	public int? NeededMemberRemovePower
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerGroupPropertyId.NeededMemberRemovePower;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return (int) val;
		}
	}

	internal ServerGroup(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~ServerGroup()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum FilePropertyId : ulong
{
	ChannelId,
	Path,
	Size,
	LastChanged,
	IsFile,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_file_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_file(IntPtr self);
}

public partial class File : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public ChannelId ChannelId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) FilePropertyId.ChannelId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public string Path
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) FilePropertyId.Path;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public long Size
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) FilePropertyId.Size;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (long) val;
		}
	}
	public DateTimeOffset LastChanged
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) FilePropertyId.LastChanged;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return DateTimeOffset.FromUnixTimeSeconds((long) val);
		}
	}
	public bool IsFile
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) FilePropertyId.IsFile;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}

	internal File(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~File()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum OptionalChannelDataPropertyId : ulong
{
	ChannelId,
	Description,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_optional_channel_data_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_optional_channel_data(IntPtr self);
}

public partial class OptionalChannelData : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public ChannelId ChannelId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalChannelDataPropertyId.ChannelId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public string Description
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalChannelDataPropertyId.Description;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}

	internal OptionalChannelData(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~OptionalChannelData()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum ChannelPropertyId : ulong
{
	Id,
	Parent,
	Name,
	Topic,
	Codec,
	CodecQuality,
	MaxClients,
	MaxFamilyClients,
	Order,
	ChannelType,
	IsDefault,
	HasPassword,
	CodecLatencyFactor,
	IsUnencrypted,
	DeleteDelay,
	NeededTalkPower,
	ForcedSilence,
	PhoneticName,
	IconId,
	IsPrivate,
	Subscribed,
	OptionalData,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_channel_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_channel(IntPtr self);
}

public partial class Channel : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public ChannelId Id
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.Id;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public ChannelId Parent
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.Parent;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public string Name
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.Name;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public string Topic
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.Topic;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public Codec? Codec
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.Codec;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return (Codec) val;
		}
	}
	public byte? CodecQuality
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.CodecQuality;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return (byte) val;
		}
	}
	public MaxClients MaxClients
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) ChannelPropertyId.MaxClients);
			return new MaxClients(basePtr, baseFunction, id);
		}
	}
	public MaxClients MaxFamilyClients
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) ChannelPropertyId.MaxFamilyClients);
			return new MaxClients(basePtr, baseFunction, id);
		}
	}
	public int Order
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.Order;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (int) val;
		}
	}
	public ChannelType ChannelType
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.ChannelType;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ChannelType) val;
		}
	}
	public bool? IsDefault
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.IsDefault;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return val != 0;
		}
	}
	public bool? HasPassword
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.HasPassword;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return val != 0;
		}
	}
	public int? CodecLatencyFactor
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.CodecLatencyFactor;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return (int) val;
		}
	}
	public bool? IsUnencrypted
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.IsUnencrypted;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return val != 0;
		}
	}
	public TimeSpan? DeleteDelay
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.DeleteDelay;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}
	public int? NeededTalkPower
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.NeededTalkPower;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return (int) val;
		}
	}
	public bool ForcedSilence
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.ForcedSilence;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public string PhoneticName
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.PhoneticName;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public IconHash? IconId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.IconId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return new IconHash { Value = val };
		}
	}
	public bool? IsPrivate
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.IsPrivate;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return val != 0;
		}
	}
	public bool Subscribed
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChannelPropertyId.Subscribed;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public OptionalChannelData OptionalData
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) ChannelPropertyId.OptionalData);
			return new OptionalChannelData(basePtr, baseFunction, id);
		}
	}

	internal Channel(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~Channel()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum OptionalClientDataPropertyId : ulong
{
	ClientId,
	Version,
	Platform,
	LoginName,
	Created,
	LastConnected,
	TotalConnection,
	MonthBytesUploaded,
	MonthBytesDownloaded,
	TotalBytesUploaded,
	TotalBytesDownloaded,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_optional_client_data_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_optional_client_data(IntPtr self);
}

public partial class OptionalClientData : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public ClientId ClientId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalClientDataPropertyId.ClientId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public string Version
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalClientDataPropertyId.Version;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public string Platform
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalClientDataPropertyId.Platform;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public string LoginName
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalClientDataPropertyId.LoginName;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public DateTimeOffset Created
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalClientDataPropertyId.Created;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return DateTimeOffset.FromUnixTimeSeconds((long) val);
		}
	}
	public DateTimeOffset LastConnected
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalClientDataPropertyId.LastConnected;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return DateTimeOffset.FromUnixTimeSeconds((long) val);
		}
	}
	public uint TotalConnection
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalClientDataPropertyId.TotalConnection;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (uint) val;
		}
	}
	public ulong MonthBytesUploaded
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalClientDataPropertyId.MonthBytesUploaded;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong MonthBytesDownloaded
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalClientDataPropertyId.MonthBytesDownloaded;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong TotalBytesUploaded
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalClientDataPropertyId.TotalBytesUploaded;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong TotalBytesDownloaded
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalClientDataPropertyId.TotalBytesDownloaded;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}

	internal OptionalClientData(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~OptionalClientData()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum ConnectionClientDataPropertyId : ulong
{
	ClientId,
	Ping,
	PingDeviation,
	ConnectedTime,
	ClientAddress,
	PacketsSentSpeech,
	PacketsSentKeepalive,
	PacketsSentControl,
	BytesSentSpeech,
	BytesSentKeepalive,
	BytesSentControl,
	PacketsReceivedSpeech,
	PacketsReceivedKeepalive,
	PacketsReceivedControl,
	BytesReceivedSpeech,
	BytesReceivedKeepalive,
	BytesReceivedControl,
	ServerToClientPacketlossSpeech,
	ServerToClientPacketlossKeepalive,
	ServerToClientPacketlossControl,
	ServerToClientPacketlossTotal,
	ClientToServerPacketlossSpeech,
	ClientToServerPacketlossKeepalive,
	ClientToServerPacketlossControl,
	ClientToServerPacketlossTotal,
	BandwidthSentLastSecondSpeech,
	BandwidthSentLastSecondKeepalive,
	BandwidthSentLastSecondControl,
	BandwidthSentLastMinuteSpeech,
	BandwidthSentLastMinuteKeepalive,
	BandwidthSentLastMinuteControl,
	BandwidthReceivedLastSecondSpeech,
	BandwidthReceivedLastSecondKeepalive,
	BandwidthReceivedLastSecondControl,
	BandwidthReceivedLastMinuteSpeech,
	BandwidthReceivedLastMinuteKeepalive,
	BandwidthReceivedLastMinuteControl,
	FiletransferBandwidthSent,
	FiletransferBandwidthReceived,
	IdleTime,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_connection_client_data_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_connection_client_data(IntPtr self);
}

public partial class ConnectionClientData : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public ClientId ClientId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.ClientId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public TimeSpan Ping
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.Ping;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}
	public TimeSpan PingDeviation
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.PingDeviation;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}
	public TimeSpan ConnectedTime
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.ConnectedTime;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}
	public IPEndPoint ClientAddress
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.ClientAddress;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return NativeMethods.ParseIPEndPoint(NativeMethods.StringFromNativeUtf8((IntPtr) val));
		}
	}
	public ulong PacketsSentSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.PacketsSentSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong PacketsSentKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.PacketsSentKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong PacketsSentControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.PacketsSentControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BytesSentSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BytesSentSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BytesSentKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BytesSentKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BytesSentControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BytesSentControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong PacketsReceivedSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.PacketsReceivedSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong PacketsReceivedKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.PacketsReceivedKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong PacketsReceivedControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.PacketsReceivedControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BytesReceivedSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BytesReceivedSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BytesReceivedKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BytesReceivedKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BytesReceivedControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BytesReceivedControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public float ServerToClientPacketlossSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.ServerToClientPacketlossSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public float ServerToClientPacketlossKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.ServerToClientPacketlossKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public float ServerToClientPacketlossControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.ServerToClientPacketlossControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public float ServerToClientPacketlossTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.ServerToClientPacketlossTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public float ClientToServerPacketlossSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.ClientToServerPacketlossSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public float ClientToServerPacketlossKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.ClientToServerPacketlossKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public float ClientToServerPacketlossControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.ClientToServerPacketlossControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public float ClientToServerPacketlossTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.ClientToServerPacketlossTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public ulong BandwidthSentLastSecondSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BandwidthSentLastSecondSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthSentLastSecondKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BandwidthSentLastSecondKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthSentLastSecondControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BandwidthSentLastSecondControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthSentLastMinuteSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BandwidthSentLastMinuteSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthSentLastMinuteKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BandwidthSentLastMinuteKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthSentLastMinuteControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BandwidthSentLastMinuteControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthReceivedLastSecondSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BandwidthReceivedLastSecondSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthReceivedLastSecondKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BandwidthReceivedLastSecondKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthReceivedLastSecondControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BandwidthReceivedLastSecondControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthReceivedLastMinuteSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BandwidthReceivedLastMinuteSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthReceivedLastMinuteKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BandwidthReceivedLastMinuteKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthReceivedLastMinuteControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.BandwidthReceivedLastMinuteControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong FiletransferBandwidthSent
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.FiletransferBandwidthSent;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong FiletransferBandwidthReceived
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.FiletransferBandwidthReceived;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public TimeSpan IdleTime
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionClientDataPropertyId.IdleTime;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}

	internal ConnectionClientData(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~ConnectionClientData()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum ClientPropertyId : ulong
{
	Id,
	Channel,
	Uid,
	Name,
	InputMuted,
	OutputMuted,
	OutputOnlyMuted,
	InputHardwareEnabled,
	OutputHardwareEnabled,
	TalkPowerGranted,
	Metadata,
	IsRecording,
	DatabaseId,
	ChannelGroup,
	ServerGroupsLen,
	ServerGroups,
	AwayMessage,
	ClientType,
	AvatarHash,
	TalkPower,
	TalkPowerRequest,
	Description,
	IsPrioritySpeaker,
	UnreadMessages,
	PhoneticName,
	NeededServerqueryViewPower,
	IconId,
	IsChannelCommander,
	CountryCode,
	InheritedChannelGroupFromChannel,
	Badges,
	OptionalData,
	ConnectionData,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_client_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_client(IntPtr self);
}

public partial class Client : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public ClientId Id
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.Id;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public ChannelId Channel
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.Channel;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public Uid Uid
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.Uid;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new Uid { Value = NativeMethods.StringFromNativeUtf8((IntPtr) val) };
		}
	}
	public string Name
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.Name;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public bool InputMuted
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.InputMuted;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public bool OutputMuted
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.OutputMuted;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public bool OutputOnlyMuted
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.OutputOnlyMuted;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public bool InputHardwareEnabled
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.InputHardwareEnabled;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public bool OutputHardwareEnabled
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.OutputHardwareEnabled;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public bool TalkPowerGranted
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.TalkPowerGranted;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public string Metadata
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.Metadata;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public bool IsRecording
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.IsRecording;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public ClientDbId DatabaseId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.DatabaseId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientDbId { Value = val };
		}
	}
	public ChannelGroupId ChannelGroup
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.ChannelGroup;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelGroupId { Value = val };
		}
	}
	public string AwayMessage
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.AwayMessage;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			if (result.typ == FfiResultType.None)
				return null;
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public ClientType ClientType
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.ClientType;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ClientType) val;
		}
	}
	public string AvatarHash
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.AvatarHash;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public int TalkPower
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.TalkPower;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (int) val;
		}
	}
	public TalkPowerRequest TalkPowerRequest
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) ClientPropertyId.TalkPowerRequest);
			return new TalkPowerRequest(basePtr, baseFunction, id);
		}
	}
	public string Description
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.Description;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public bool IsPrioritySpeaker
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.IsPrioritySpeaker;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public uint UnreadMessages
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.UnreadMessages;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (uint) val;
		}
	}
	public string PhoneticName
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.PhoneticName;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public int NeededServerqueryViewPower
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.NeededServerqueryViewPower;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (int) val;
		}
	}
	public IconHash IconId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.IconId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new IconHash { Value = val };
		}
	}
	public bool IsChannelCommander
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.IsChannelCommander;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public string CountryCode
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.CountryCode;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public ChannelId InheritedChannelGroupFromChannel
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.InheritedChannelGroupFromChannel;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelId { Value = val };
		}
	}
	public string Badges
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ClientPropertyId.Badges;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public OptionalClientData OptionalData
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) ClientPropertyId.OptionalData);
			return new OptionalClientData(basePtr, baseFunction, id);
		}
	}
	public ConnectionClientData ConnectionData
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) ClientPropertyId.ConnectionData);
			return new ConnectionClientData(basePtr, baseFunction, id);
		}
	}

	internal Client(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~Client()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum OptionalServerDataPropertyId : ulong
{
	ConnectionCount,
	ChannelCount,
	Uptime,
	HasPassword,
	DefaultChannelAdminGroup,
	MaxDownloadTotalBandwith,
	MaxUploadTotalBandwith,
	ComplainAutobanCount,
	ComplainAutobanTime,
	ComplainRemoveTime,
	MinClientsForceSilence,
	AntifloodPointsTickReduce,
	AntifloodPointsNeededCommandBlock,
	ClientCount,
	QueryCount,
	QueryOnlineCount,
	DownloadQuota,
	UploadQuota,
	MonthBytesDownloaded,
	MonthBytesUploaded,
	TotalBytesDownloaded,
	TotalBytesUploaded,
	Port,
	Autostart,
	MachineId,
	NeededIdentitySecurityLevel,
	LogClient,
	LogQuery,
	LogChannel,
	LogPermissions,
	LogServer,
	LogFileTransfer,
	MinClientVersion,
	ReservedSlots,
	TotalPacketlossSpeech,
	TotalPacketlossKeepalive,
	TotalPacketlossControl,
	TotalPacketlossTotal,
	TotalPing,
	WeblistEnabled,
	MinAndroidVersion,
	MinIosVersion,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_optional_server_data_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_optional_server_data(IntPtr self);
}

public partial class OptionalServerData : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public uint ConnectionCount
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.ConnectionCount;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (uint) val;
		}
	}
	public ulong ChannelCount
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.ChannelCount;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public TimeSpan Uptime
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.Uptime;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}
	public bool HasPassword
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.HasPassword;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public ChannelGroupId DefaultChannelAdminGroup
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.DefaultChannelAdminGroup;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelGroupId { Value = val };
		}
	}
	public ulong MaxDownloadTotalBandwith
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.MaxDownloadTotalBandwith;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong MaxUploadTotalBandwith
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.MaxUploadTotalBandwith;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public uint ComplainAutobanCount
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.ComplainAutobanCount;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (uint) val;
		}
	}
	public TimeSpan ComplainAutobanTime
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.ComplainAutobanTime;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}
	public TimeSpan ComplainRemoveTime
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.ComplainRemoveTime;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}
	public ushort MinClientsForceSilence
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.MinClientsForceSilence;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ushort) val;
		}
	}
	public uint AntifloodPointsTickReduce
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.AntifloodPointsTickReduce;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (uint) val;
		}
	}
	public uint AntifloodPointsNeededCommandBlock
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.AntifloodPointsNeededCommandBlock;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (uint) val;
		}
	}
	public ushort ClientCount
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.ClientCount;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ushort) val;
		}
	}
	public uint QueryCount
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.QueryCount;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (uint) val;
		}
	}
	public uint QueryOnlineCount
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.QueryOnlineCount;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (uint) val;
		}
	}
	public ulong DownloadQuota
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.DownloadQuota;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong UploadQuota
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.UploadQuota;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong MonthBytesDownloaded
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.MonthBytesDownloaded;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong MonthBytesUploaded
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.MonthBytesUploaded;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong TotalBytesDownloaded
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.TotalBytesDownloaded;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong TotalBytesUploaded
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.TotalBytesUploaded;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ushort Port
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.Port;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ushort) val;
		}
	}
	public bool Autostart
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.Autostart;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public string MachineId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.MachineId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public byte NeededIdentitySecurityLevel
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.NeededIdentitySecurityLevel;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (byte) val;
		}
	}
	public bool LogClient
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.LogClient;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public bool LogQuery
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.LogQuery;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public bool LogChannel
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.LogChannel;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public bool LogPermissions
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.LogPermissions;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public bool LogServer
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.LogServer;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public bool LogFileTransfer
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.LogFileTransfer;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public DateTimeOffset MinClientVersion
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.MinClientVersion;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return DateTimeOffset.FromUnixTimeSeconds((long) val);
		}
	}
	public ushort ReservedSlots
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.ReservedSlots;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ushort) val;
		}
	}
	public float TotalPacketlossSpeech
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.TotalPacketlossSpeech;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public float TotalPacketlossKeepalive
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.TotalPacketlossKeepalive;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public float TotalPacketlossControl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.TotalPacketlossControl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public float TotalPacketlossTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.TotalPacketlossTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public TimeSpan TotalPing
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.TotalPing;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}
	public bool WeblistEnabled
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.WeblistEnabled;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public DateTimeOffset MinAndroidVersion
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.MinAndroidVersion;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return DateTimeOffset.FromUnixTimeSeconds((long) val);
		}
	}
	public DateTimeOffset MinIosVersion
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) OptionalServerDataPropertyId.MinIosVersion;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return DateTimeOffset.FromUnixTimeSeconds((long) val);
		}
	}

	internal OptionalServerData(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~OptionalServerData()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum ConnectionServerDataPropertyId : ulong
{
	FileTransferBandwidthSent,
	FileTransferBandwidthReceived,
	FileTransferBytesSentTotal,
	FileTransferBytesReceivedTotal,
	PacketsSentTotal,
	BytesSentTotal,
	PacketsReceivedTotal,
	BytesReceivedTotal,
	BandwidthSentLastSecondTotal,
	BandwidthSentLastMinuteTotal,
	BandwidthReceivedLastSecondTotal,
	BandwidthReceivedLastMinuteTotal,
	ConnectedTime,
	PacketlossTotal,
	Ping,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_connection_server_data_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_connection_server_data(IntPtr self);
}

public partial class ConnectionServerData : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public ulong FileTransferBandwidthSent
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.FileTransferBandwidthSent;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong FileTransferBandwidthReceived
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.FileTransferBandwidthReceived;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong FileTransferBytesSentTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.FileTransferBytesSentTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong FileTransferBytesReceivedTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.FileTransferBytesReceivedTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong PacketsSentTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.PacketsSentTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BytesSentTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.BytesSentTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong PacketsReceivedTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.PacketsReceivedTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BytesReceivedTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.BytesReceivedTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthSentLastSecondTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.BandwidthSentLastSecondTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthSentLastMinuteTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.BandwidthSentLastMinuteTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthReceivedLastSecondTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.BandwidthReceivedLastSecondTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public ulong BandwidthReceivedLastMinuteTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.BandwidthReceivedLastMinuteTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public TimeSpan ConnectedTime
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.ConnectedTime;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}
	public float PacketlossTotal
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.PacketlossTotal;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public TimeSpan Ping
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionServerDataPropertyId.Ping;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}

	internal ConnectionServerData(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~ConnectionServerData()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum ServerPropertyId : ulong
{
	Uid,
	VirtualServerId,
	Name,
	WelcomeMessage,
	Platform,
	Version,
	MaxClients,
	Created,
	CodecEncryptionMode,
	Hostmessage,
	HostmessageMode,
	DefaultServerGroup,
	DefaultChannelGroup,
	HostbannerUrl,
	HostbannerGfxUrl,
	HostbannerGfxInterval,
	PrioritySpeakerDimmModificator,
	HostbuttonTooltip,
	HostbuttonUrl,
	HostbuttonGfxUrl,
	PhoneticName,
	IconId,
	IpsLen,
	Ips,
	AskForPrivilegekey,
	HostbannerMode,
	TempChannelDefaultDeleteDelay,
	ProtocolVersion,
	License,
	OptionalData,
	ConnectionData,
	ClientsLen,
	Clients,
	ChannelsLen,
	Channels,
	GroupsLen,
	Groups,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_server_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_server(IntPtr self);
}

public partial class Server : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public Uid Uid
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.Uid;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new Uid { Value = NativeMethods.StringFromNativeUtf8((IntPtr) val) };
		}
	}
	public ulong VirtualServerId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.VirtualServerId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ulong) val;
		}
	}
	public string Name
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.Name;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public string WelcomeMessage
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.WelcomeMessage;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public string Platform
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.Platform;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public string Version
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.Version;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public ushort MaxClients
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.MaxClients;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ushort) val;
		}
	}
	public DateTimeOffset Created
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.Created;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return DateTimeOffset.FromUnixTimeSeconds((long) val);
		}
	}
	public CodecEncryptionMode CodecEncryptionMode
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.CodecEncryptionMode;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (CodecEncryptionMode) val;
		}
	}
	public string Hostmessage
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.Hostmessage;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public HostMessageMode HostmessageMode
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.HostmessageMode;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (HostMessageMode) val;
		}
	}
	public ServerGroupId DefaultServerGroup
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.DefaultServerGroup;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ServerGroupId { Value = val };
		}
	}
	public ChannelGroupId DefaultChannelGroup
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.DefaultChannelGroup;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ChannelGroupId { Value = val };
		}
	}
	public string HostbannerUrl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.HostbannerUrl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public string HostbannerGfxUrl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.HostbannerGfxUrl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public TimeSpan HostbannerGfxInterval
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.HostbannerGfxInterval;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}
	public float PrioritySpeakerDimmModificator
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.PrioritySpeakerDimmModificator;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new FloatConverter { U64Value = val }.F32Value;
		}
	}
	public string HostbuttonTooltip
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.HostbuttonTooltip;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public string HostbuttonUrl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.HostbuttonUrl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public string HostbuttonGfxUrl
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.HostbuttonGfxUrl;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public string PhoneticName
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.PhoneticName;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public IconHash IconId
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.IconId;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new IconHash { Value = val };
		}
	}
	public bool AskForPrivilegekey
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.AskForPrivilegekey;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return val != 0;
		}
	}
	public HostBannerMode HostbannerMode
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.HostbannerMode;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (HostBannerMode) val;
		}
	}
	public TimeSpan TempChannelDefaultDeleteDelay
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.TempChannelDefaultDeleteDelay;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new TimeSpan((long) val * 10000000);
		}
	}
	public ushort ProtocolVersion
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.ProtocolVersion;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (ushort) val;
		}
	}
	public LicenseType License
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ServerPropertyId.License;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (LicenseType) val;
		}
	}
	public OptionalServerData OptionalData
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) ServerPropertyId.OptionalData);
			return new OptionalServerData(basePtr, baseFunction, id);
		}
	}
	public ConnectionServerData ConnectionData
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) ServerPropertyId.ConnectionData);
			return new ConnectionServerData(basePtr, baseFunction, id);
		}
	}
	public FakeDictionary<ClientId, Client> Clients
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) ServerPropertyId.Clients);
			return new FakeDictionary<ClientId, Client>(basePtr, baseFunction, id)
			{
				containsObjects = true,
				convertKey = val => (ulong) val.Value,
				convertKeyArray = (list, length) => { unsafe {
					var lis = (ulong*) list;
					var res = new List<ClientId>((int) length);
					for (ulong i = 0; i < length; i++)
					{
						var val = *(lis + i);
						res.Add(new ClientId { Value = (ushort) val });
					}
					NativeMethods.tscl_free_u64s(lis, (UIntPtr) length);
					return res;
				} },
				createValue = (basePtr, baseFunction, i) =>
					new Client(basePtr, baseFunction, i),
			};
		}
	}
	public FakeDictionary<ChannelId, Channel> Channels
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) ServerPropertyId.Channels);
			return new FakeDictionary<ChannelId, Channel>(basePtr, baseFunction, id)
			{
				containsObjects = true,
				convertKey = val => val.Value,
				convertKeyArray = (list, length) => { unsafe {
					var lis = (ulong*) list;
					var res = new List<ChannelId>((int) length);
					for (ulong i = 0; i < length; i++)
					{
						var val = *(lis + i);
						res.Add(new ChannelId { Value = val });
					}
					NativeMethods.tscl_free_u64s(lis, (UIntPtr) length);
					return res;
				} },
				createValue = (basePtr, baseFunction, i) =>
					new Channel(basePtr, baseFunction, i),
			};
		}
	}
	public FakeDictionary<ServerGroupId, ServerGroup> Groups
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) ServerPropertyId.Groups);
			return new FakeDictionary<ServerGroupId, ServerGroup>(basePtr, baseFunction, id)
			{
				containsObjects = true,
				convertKey = val => val.Value,
				convertKeyArray = (list, length) => { unsafe {
					var lis = (ulong*) list;
					var res = new List<ServerGroupId>((int) length);
					for (ulong i = 0; i < length; i++)
					{
						var val = *(lis + i);
						res.Add(new ServerGroupId { Value = val });
					}
					NativeMethods.tscl_free_u64s(lis, (UIntPtr) length);
					return res;
				} },
				createValue = (basePtr, baseFunction, i) =>
					new ServerGroup(basePtr, baseFunction, i),
			};
		}
	}

	internal Server(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~Server()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum ConnectionPropertyId : ulong
{
	OwnClient,
	Server,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_connection_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_connection(IntPtr self);
}

public partial class Connection : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public ClientId OwnClient
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ConnectionPropertyId.OwnClient;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public Server Server
	{
		get
		{
			List<ulong> id = new List<ulong>(this.id);
			id.Add((ulong) ConnectionPropertyId.Server);
			return new Server(basePtr, baseFunction, id);
		}
	}

	internal Connection(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~Connection()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}

internal enum ChatEntryPropertyId : ulong
{
	SenderClient,
	Text,
	Date,
	Mode,
}

internal static partial class NativeMethods
{
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_chat_entry_get(IntPtr self,
		UIntPtr idLen, ulong* id, [Out] out FfiResult result);
	[DllImport(libPath, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
	public static extern unsafe void tscl_free_chat_entry(IntPtr self);
}

public partial class ChatEntry : INotifyPropertyChanged
{
	internal IntPtr basePtr;
	internal NativeMethods.FfiFunction baseFunction;
	internal List<ulong> id;

	public event PropertyChangedEventHandler PropertyChanged;
	private Action<string> onChangeListener;

	public ClientId SenderClient
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChatEntryPropertyId.SenderClient;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return new ClientId { Value = (ushort) val };
		}
	}
	public string Text
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChatEntryPropertyId.Text;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return NativeMethods.StringFromNativeUtf8((IntPtr) val);
		}
	}
	public DateTimeOffset Date
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChatEntryPropertyId.Date;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return DateTimeOffset.FromUnixTimeSeconds((long) val);
		}
	}
	public TextMessageTargetMode Mode
	{
		get
		{
			ulong[] id = new ulong[this.id.Count + 1];
			this.id.CopyTo(id, 0);
			id[this.id.Count] = (ulong) ChatEntryPropertyId.Mode;
			FfiResult result;
			unsafe { fixed (ulong* ids = id)
			{
				baseFunction(basePtr, (UIntPtr) id.Length, ids, out result);
			} }
			if (result.typ == FfiResultType.Error)
				throw new Exception(NativeMethods.StringFromNativeUtf8((IntPtr) result.content));
			ulong val = result.content;
			return (TextMessageTargetMode) val;
		}
	}

	internal ChatEntry(IntPtr basePtr, NativeMethods.FfiFunction baseFunction, List<ulong> id)
	{
		this.basePtr = basePtr;
		this.baseFunction = baseFunction;
		this.id = id;

		bool shouldListen;
		unsafe
		{
			shouldListen = basePtr == IntPtr.Zero && baseFunction == NativeMethods.tscl_connection_by_id;
		}
		if (shouldListen)
		{
			onChangeListener = name => NotifyPropertyChanged(name);
			Core.AddOnChangeListener(this.id.ToArray(), onChangeListener);
		}
	}

	~ChatEntry()
	{
		if (onChangeListener != null)
			Core.RemoveOnChangeListener(this.id.ToArray(), onChangeListener);
	}

	internal void NotifyPropertyChanged([CallerMemberName] String propertyName = "")
	{
		if (PropertyChanged != null)
			PropertyChanged(this, new PropertyChangedEventArgs(propertyName));
	}
}
}


namespace Qint.Wrapper
{
	internal static partial class NativeMethods
	{
		public static string PropertyIdToName(PropertyId id)
		{
			switch (id.Type)
			{
			case PropertyIdType.ServerGroup:
				return "ServerGroup";
			case PropertyIdType.File:
				return "File";
			case PropertyIdType.OptionalChannelData:
				return "OptionalChannelData";
			case PropertyIdType.Channel:
				return "Channel";
			case PropertyIdType.OptionalClientData:
				return "OptionalClientData";
			case PropertyIdType.ConnectionClientData:
				return "ConnectionClientData";
			case PropertyIdType.Client:
				return "Client";
			case PropertyIdType.OptionalServerData:
				return "OptionalServerData";
			case PropertyIdType.ConnectionServerData:
				return "ConnectionServerData";
			case PropertyIdType.Server:
				return "Server";
			case PropertyIdType.Connection:
				return "Connection";
			case PropertyIdType.ChatEntry:
				return "ChatEntry";

			case PropertyIdType.ServerGroupId:
				return "ServerGroupId";
			case PropertyIdType.ServerGroupName:
				return "ServerGroupName";
			case PropertyIdType.ServerGroupGroupType:
				return "ServerGroupGroupType";
			case PropertyIdType.ServerGroupIconId:
				return "ServerGroupIconId";
			case PropertyIdType.ServerGroupIsPermanent:
				return "ServerGroupIsPermanent";
			case PropertyIdType.ServerGroupSortId:
				return "ServerGroupSortId";
			case PropertyIdType.ServerGroupNamingMode:
				return "ServerGroupNamingMode";
			case PropertyIdType.ServerGroupNeededModifyPower:
				return "ServerGroupNeededModifyPower";
			case PropertyIdType.ServerGroupNeededMemberAddPower:
				return "ServerGroupNeededMemberAddPower";
			case PropertyIdType.ServerGroupNeededMemberRemovePower:
				return "ServerGroupNeededMemberRemovePower";
			case PropertyIdType.FilePath:
				return "FilePath";
			case PropertyIdType.FileSize:
				return "FileSize";
			case PropertyIdType.FileLastChanged:
				return "FileLastChanged";
			case PropertyIdType.FileIsFile:
				return "FileIsFile";
			case PropertyIdType.OptionalChannelDataDescription:
				return "OptionalChannelDataDescription";
			case PropertyIdType.ChannelId:
				return "ChannelId";
			case PropertyIdType.ChannelParent:
				return "ChannelParent";
			case PropertyIdType.ChannelName:
				return "ChannelName";
			case PropertyIdType.ChannelTopic:
				return "ChannelTopic";
			case PropertyIdType.ChannelCodec:
				return "ChannelCodec";
			case PropertyIdType.ChannelCodecQuality:
				return "ChannelCodecQuality";
			case PropertyIdType.ChannelMaxClients:
				return "ChannelMaxClients";
			case PropertyIdType.ChannelMaxFamilyClients:
				return "ChannelMaxFamilyClients";
			case PropertyIdType.ChannelOrder:
				return "ChannelOrder";
			case PropertyIdType.ChannelChannelType:
				return "ChannelChannelType";
			case PropertyIdType.ChannelIsDefault:
				return "ChannelIsDefault";
			case PropertyIdType.ChannelHasPassword:
				return "ChannelHasPassword";
			case PropertyIdType.ChannelCodecLatencyFactor:
				return "ChannelCodecLatencyFactor";
			case PropertyIdType.ChannelIsUnencrypted:
				return "ChannelIsUnencrypted";
			case PropertyIdType.ChannelDeleteDelay:
				return "ChannelDeleteDelay";
			case PropertyIdType.ChannelNeededTalkPower:
				return "ChannelNeededTalkPower";
			case PropertyIdType.ChannelForcedSilence:
				return "ChannelForcedSilence";
			case PropertyIdType.ChannelPhoneticName:
				return "ChannelPhoneticName";
			case PropertyIdType.ChannelIconId:
				return "ChannelIconId";
			case PropertyIdType.ChannelIsPrivate:
				return "ChannelIsPrivate";
			case PropertyIdType.ChannelSubscribed:
				return "ChannelSubscribed";
			case PropertyIdType.OptionalClientDataVersion:
				return "OptionalClientDataVersion";
			case PropertyIdType.OptionalClientDataPlatform:
				return "OptionalClientDataPlatform";
			case PropertyIdType.OptionalClientDataLoginName:
				return "OptionalClientDataLoginName";
			case PropertyIdType.OptionalClientDataCreated:
				return "OptionalClientDataCreated";
			case PropertyIdType.OptionalClientDataLastConnected:
				return "OptionalClientDataLastConnected";
			case PropertyIdType.OptionalClientDataTotalConnection:
				return "OptionalClientDataTotalConnection";
			case PropertyIdType.OptionalClientDataMonthBytesUploaded:
				return "OptionalClientDataMonthBytesUploaded";
			case PropertyIdType.OptionalClientDataMonthBytesDownloaded:
				return "OptionalClientDataMonthBytesDownloaded";
			case PropertyIdType.OptionalClientDataTotalBytesUploaded:
				return "OptionalClientDataTotalBytesUploaded";
			case PropertyIdType.OptionalClientDataTotalBytesDownloaded:
				return "OptionalClientDataTotalBytesDownloaded";
			case PropertyIdType.ConnectionClientDataPing:
				return "ConnectionClientDataPing";
			case PropertyIdType.ConnectionClientDataPingDeviation:
				return "ConnectionClientDataPingDeviation";
			case PropertyIdType.ConnectionClientDataConnectedTime:
				return "ConnectionClientDataConnectedTime";
			case PropertyIdType.ConnectionClientDataClientAddress:
				return "ConnectionClientDataClientAddress";
			case PropertyIdType.ConnectionClientDataPacketsSentSpeech:
				return "ConnectionClientDataPacketsSentSpeech";
			case PropertyIdType.ConnectionClientDataPacketsSentKeepalive:
				return "ConnectionClientDataPacketsSentKeepalive";
			case PropertyIdType.ConnectionClientDataPacketsSentControl:
				return "ConnectionClientDataPacketsSentControl";
			case PropertyIdType.ConnectionClientDataBytesSentSpeech:
				return "ConnectionClientDataBytesSentSpeech";
			case PropertyIdType.ConnectionClientDataBytesSentKeepalive:
				return "ConnectionClientDataBytesSentKeepalive";
			case PropertyIdType.ConnectionClientDataBytesSentControl:
				return "ConnectionClientDataBytesSentControl";
			case PropertyIdType.ConnectionClientDataPacketsReceivedSpeech:
				return "ConnectionClientDataPacketsReceivedSpeech";
			case PropertyIdType.ConnectionClientDataPacketsReceivedKeepalive:
				return "ConnectionClientDataPacketsReceivedKeepalive";
			case PropertyIdType.ConnectionClientDataPacketsReceivedControl:
				return "ConnectionClientDataPacketsReceivedControl";
			case PropertyIdType.ConnectionClientDataBytesReceivedSpeech:
				return "ConnectionClientDataBytesReceivedSpeech";
			case PropertyIdType.ConnectionClientDataBytesReceivedKeepalive:
				return "ConnectionClientDataBytesReceivedKeepalive";
			case PropertyIdType.ConnectionClientDataBytesReceivedControl:
				return "ConnectionClientDataBytesReceivedControl";
			case PropertyIdType.ConnectionClientDataServerToClientPacketlossSpeech:
				return "ConnectionClientDataServerToClientPacketlossSpeech";
			case PropertyIdType.ConnectionClientDataServerToClientPacketlossKeepalive:
				return "ConnectionClientDataServerToClientPacketlossKeepalive";
			case PropertyIdType.ConnectionClientDataServerToClientPacketlossControl:
				return "ConnectionClientDataServerToClientPacketlossControl";
			case PropertyIdType.ConnectionClientDataServerToClientPacketlossTotal:
				return "ConnectionClientDataServerToClientPacketlossTotal";
			case PropertyIdType.ConnectionClientDataClientToServerPacketlossSpeech:
				return "ConnectionClientDataClientToServerPacketlossSpeech";
			case PropertyIdType.ConnectionClientDataClientToServerPacketlossKeepalive:
				return "ConnectionClientDataClientToServerPacketlossKeepalive";
			case PropertyIdType.ConnectionClientDataClientToServerPacketlossControl:
				return "ConnectionClientDataClientToServerPacketlossControl";
			case PropertyIdType.ConnectionClientDataClientToServerPacketlossTotal:
				return "ConnectionClientDataClientToServerPacketlossTotal";
			case PropertyIdType.ConnectionClientDataBandwidthSentLastSecondSpeech:
				return "ConnectionClientDataBandwidthSentLastSecondSpeech";
			case PropertyIdType.ConnectionClientDataBandwidthSentLastSecondKeepalive:
				return "ConnectionClientDataBandwidthSentLastSecondKeepalive";
			case PropertyIdType.ConnectionClientDataBandwidthSentLastSecondControl:
				return "ConnectionClientDataBandwidthSentLastSecondControl";
			case PropertyIdType.ConnectionClientDataBandwidthSentLastMinuteSpeech:
				return "ConnectionClientDataBandwidthSentLastMinuteSpeech";
			case PropertyIdType.ConnectionClientDataBandwidthSentLastMinuteKeepalive:
				return "ConnectionClientDataBandwidthSentLastMinuteKeepalive";
			case PropertyIdType.ConnectionClientDataBandwidthSentLastMinuteControl:
				return "ConnectionClientDataBandwidthSentLastMinuteControl";
			case PropertyIdType.ConnectionClientDataBandwidthReceivedLastSecondSpeech:
				return "ConnectionClientDataBandwidthReceivedLastSecondSpeech";
			case PropertyIdType.ConnectionClientDataBandwidthReceivedLastSecondKeepalive:
				return "ConnectionClientDataBandwidthReceivedLastSecondKeepalive";
			case PropertyIdType.ConnectionClientDataBandwidthReceivedLastSecondControl:
				return "ConnectionClientDataBandwidthReceivedLastSecondControl";
			case PropertyIdType.ConnectionClientDataBandwidthReceivedLastMinuteSpeech:
				return "ConnectionClientDataBandwidthReceivedLastMinuteSpeech";
			case PropertyIdType.ConnectionClientDataBandwidthReceivedLastMinuteKeepalive:
				return "ConnectionClientDataBandwidthReceivedLastMinuteKeepalive";
			case PropertyIdType.ConnectionClientDataBandwidthReceivedLastMinuteControl:
				return "ConnectionClientDataBandwidthReceivedLastMinuteControl";
			case PropertyIdType.ConnectionClientDataFiletransferBandwidthSent:
				return "ConnectionClientDataFiletransferBandwidthSent";
			case PropertyIdType.ConnectionClientDataFiletransferBandwidthReceived:
				return "ConnectionClientDataFiletransferBandwidthReceived";
			case PropertyIdType.ConnectionClientDataIdleTime:
				return "ConnectionClientDataIdleTime";
			case PropertyIdType.ClientId:
				return "ClientId";
			case PropertyIdType.ClientChannel:
				return "ClientChannel";
			case PropertyIdType.ClientUid:
				return "ClientUid";
			case PropertyIdType.ClientName:
				return "ClientName";
			case PropertyIdType.ClientInputMuted:
				return "ClientInputMuted";
			case PropertyIdType.ClientOutputMuted:
				return "ClientOutputMuted";
			case PropertyIdType.ClientOutputOnlyMuted:
				return "ClientOutputOnlyMuted";
			case PropertyIdType.ClientInputHardwareEnabled:
				return "ClientInputHardwareEnabled";
			case PropertyIdType.ClientOutputHardwareEnabled:
				return "ClientOutputHardwareEnabled";
			case PropertyIdType.ClientTalkPowerGranted:
				return "ClientTalkPowerGranted";
			case PropertyIdType.ClientMetadata:
				return "ClientMetadata";
			case PropertyIdType.ClientIsRecording:
				return "ClientIsRecording";
			case PropertyIdType.ClientDatabaseId:
				return "ClientDatabaseId";
			case PropertyIdType.ClientChannelGroup:
				return "ClientChannelGroup";
			case PropertyIdType.ClientServerGroup:
				return "ClientServerGroup";
			case PropertyIdType.ClientAwayMessage:
				return "ClientAwayMessage";
			case PropertyIdType.ClientClientType:
				return "ClientClientType";
			case PropertyIdType.ClientAvatarHash:
				return "ClientAvatarHash";
			case PropertyIdType.ClientTalkPower:
				return "ClientTalkPower";
			case PropertyIdType.ClientTalkPowerRequest:
				return "ClientTalkPowerRequest";
			case PropertyIdType.ClientDescription:
				return "ClientDescription";
			case PropertyIdType.ClientIsPrioritySpeaker:
				return "ClientIsPrioritySpeaker";
			case PropertyIdType.ClientUnreadMessages:
				return "ClientUnreadMessages";
			case PropertyIdType.ClientPhoneticName:
				return "ClientPhoneticName";
			case PropertyIdType.ClientNeededServerqueryViewPower:
				return "ClientNeededServerqueryViewPower";
			case PropertyIdType.ClientIconId:
				return "ClientIconId";
			case PropertyIdType.ClientIsChannelCommander:
				return "ClientIsChannelCommander";
			case PropertyIdType.ClientCountryCode:
				return "ClientCountryCode";
			case PropertyIdType.ClientInheritedChannelGroupFromChannel:
				return "ClientInheritedChannelGroupFromChannel";
			case PropertyIdType.ClientBadges:
				return "ClientBadges";
			case PropertyIdType.OptionalServerDataConnectionCount:
				return "OptionalServerDataConnectionCount";
			case PropertyIdType.OptionalServerDataChannelCount:
				return "OptionalServerDataChannelCount";
			case PropertyIdType.OptionalServerDataUptime:
				return "OptionalServerDataUptime";
			case PropertyIdType.OptionalServerDataHasPassword:
				return "OptionalServerDataHasPassword";
			case PropertyIdType.OptionalServerDataDefaultChannelAdminGroup:
				return "OptionalServerDataDefaultChannelAdminGroup";
			case PropertyIdType.OptionalServerDataMaxDownloadTotalBandwith:
				return "OptionalServerDataMaxDownloadTotalBandwith";
			case PropertyIdType.OptionalServerDataMaxUploadTotalBandwith:
				return "OptionalServerDataMaxUploadTotalBandwith";
			case PropertyIdType.OptionalServerDataComplainAutobanCount:
				return "OptionalServerDataComplainAutobanCount";
			case PropertyIdType.OptionalServerDataComplainAutobanTime:
				return "OptionalServerDataComplainAutobanTime";
			case PropertyIdType.OptionalServerDataComplainRemoveTime:
				return "OptionalServerDataComplainRemoveTime";
			case PropertyIdType.OptionalServerDataMinClientsForceSilence:
				return "OptionalServerDataMinClientsForceSilence";
			case PropertyIdType.OptionalServerDataAntifloodPointsTickReduce:
				return "OptionalServerDataAntifloodPointsTickReduce";
			case PropertyIdType.OptionalServerDataAntifloodPointsNeededCommandBlock:
				return "OptionalServerDataAntifloodPointsNeededCommandBlock";
			case PropertyIdType.OptionalServerDataClientCount:
				return "OptionalServerDataClientCount";
			case PropertyIdType.OptionalServerDataQueryCount:
				return "OptionalServerDataQueryCount";
			case PropertyIdType.OptionalServerDataQueryOnlineCount:
				return "OptionalServerDataQueryOnlineCount";
			case PropertyIdType.OptionalServerDataDownloadQuota:
				return "OptionalServerDataDownloadQuota";
			case PropertyIdType.OptionalServerDataUploadQuota:
				return "OptionalServerDataUploadQuota";
			case PropertyIdType.OptionalServerDataMonthBytesDownloaded:
				return "OptionalServerDataMonthBytesDownloaded";
			case PropertyIdType.OptionalServerDataMonthBytesUploaded:
				return "OptionalServerDataMonthBytesUploaded";
			case PropertyIdType.OptionalServerDataTotalBytesDownloaded:
				return "OptionalServerDataTotalBytesDownloaded";
			case PropertyIdType.OptionalServerDataTotalBytesUploaded:
				return "OptionalServerDataTotalBytesUploaded";
			case PropertyIdType.OptionalServerDataPort:
				return "OptionalServerDataPort";
			case PropertyIdType.OptionalServerDataAutostart:
				return "OptionalServerDataAutostart";
			case PropertyIdType.OptionalServerDataMachineId:
				return "OptionalServerDataMachineId";
			case PropertyIdType.OptionalServerDataNeededIdentitySecurityLevel:
				return "OptionalServerDataNeededIdentitySecurityLevel";
			case PropertyIdType.OptionalServerDataLogClient:
				return "OptionalServerDataLogClient";
			case PropertyIdType.OptionalServerDataLogQuery:
				return "OptionalServerDataLogQuery";
			case PropertyIdType.OptionalServerDataLogChannel:
				return "OptionalServerDataLogChannel";
			case PropertyIdType.OptionalServerDataLogPermissions:
				return "OptionalServerDataLogPermissions";
			case PropertyIdType.OptionalServerDataLogServer:
				return "OptionalServerDataLogServer";
			case PropertyIdType.OptionalServerDataLogFileTransfer:
				return "OptionalServerDataLogFileTransfer";
			case PropertyIdType.OptionalServerDataMinClientVersion:
				return "OptionalServerDataMinClientVersion";
			case PropertyIdType.OptionalServerDataReservedSlots:
				return "OptionalServerDataReservedSlots";
			case PropertyIdType.OptionalServerDataTotalPacketlossSpeech:
				return "OptionalServerDataTotalPacketlossSpeech";
			case PropertyIdType.OptionalServerDataTotalPacketlossKeepalive:
				return "OptionalServerDataTotalPacketlossKeepalive";
			case PropertyIdType.OptionalServerDataTotalPacketlossControl:
				return "OptionalServerDataTotalPacketlossControl";
			case PropertyIdType.OptionalServerDataTotalPacketlossTotal:
				return "OptionalServerDataTotalPacketlossTotal";
			case PropertyIdType.OptionalServerDataTotalPing:
				return "OptionalServerDataTotalPing";
			case PropertyIdType.OptionalServerDataWeblistEnabled:
				return "OptionalServerDataWeblistEnabled";
			case PropertyIdType.OptionalServerDataMinAndroidVersion:
				return "OptionalServerDataMinAndroidVersion";
			case PropertyIdType.OptionalServerDataMinIosVersion:
				return "OptionalServerDataMinIosVersion";
			case PropertyIdType.ConnectionServerDataFileTransferBandwidthSent:
				return "ConnectionServerDataFileTransferBandwidthSent";
			case PropertyIdType.ConnectionServerDataFileTransferBandwidthReceived:
				return "ConnectionServerDataFileTransferBandwidthReceived";
			case PropertyIdType.ConnectionServerDataFileTransferBytesSentTotal:
				return "ConnectionServerDataFileTransferBytesSentTotal";
			case PropertyIdType.ConnectionServerDataFileTransferBytesReceivedTotal:
				return "ConnectionServerDataFileTransferBytesReceivedTotal";
			case PropertyIdType.ConnectionServerDataPacketsSentTotal:
				return "ConnectionServerDataPacketsSentTotal";
			case PropertyIdType.ConnectionServerDataBytesSentTotal:
				return "ConnectionServerDataBytesSentTotal";
			case PropertyIdType.ConnectionServerDataPacketsReceivedTotal:
				return "ConnectionServerDataPacketsReceivedTotal";
			case PropertyIdType.ConnectionServerDataBytesReceivedTotal:
				return "ConnectionServerDataBytesReceivedTotal";
			case PropertyIdType.ConnectionServerDataBandwidthSentLastSecondTotal:
				return "ConnectionServerDataBandwidthSentLastSecondTotal";
			case PropertyIdType.ConnectionServerDataBandwidthSentLastMinuteTotal:
				return "ConnectionServerDataBandwidthSentLastMinuteTotal";
			case PropertyIdType.ConnectionServerDataBandwidthReceivedLastSecondTotal:
				return "ConnectionServerDataBandwidthReceivedLastSecondTotal";
			case PropertyIdType.ConnectionServerDataBandwidthReceivedLastMinuteTotal:
				return "ConnectionServerDataBandwidthReceivedLastMinuteTotal";
			case PropertyIdType.ConnectionServerDataConnectedTime:
				return "ConnectionServerDataConnectedTime";
			case PropertyIdType.ConnectionServerDataPacketlossTotal:
				return "ConnectionServerDataPacketlossTotal";
			case PropertyIdType.ConnectionServerDataPing:
				return "ConnectionServerDataPing";
			case PropertyIdType.ServerUid:
				return "ServerUid";
			case PropertyIdType.ServerVirtualServerId:
				return "ServerVirtualServerId";
			case PropertyIdType.ServerName:
				return "ServerName";
			case PropertyIdType.ServerWelcomeMessage:
				return "ServerWelcomeMessage";
			case PropertyIdType.ServerPlatform:
				return "ServerPlatform";
			case PropertyIdType.ServerVersion:
				return "ServerVersion";
			case PropertyIdType.ServerMaxClients:
				return "ServerMaxClients";
			case PropertyIdType.ServerCreated:
				return "ServerCreated";
			case PropertyIdType.ServerCodecEncryptionMode:
				return "ServerCodecEncryptionMode";
			case PropertyIdType.ServerHostmessage:
				return "ServerHostmessage";
			case PropertyIdType.ServerHostmessageMode:
				return "ServerHostmessageMode";
			case PropertyIdType.ServerDefaultServerGroup:
				return "ServerDefaultServerGroup";
			case PropertyIdType.ServerDefaultChannelGroup:
				return "ServerDefaultChannelGroup";
			case PropertyIdType.ServerHostbannerUrl:
				return "ServerHostbannerUrl";
			case PropertyIdType.ServerHostbannerGfxUrl:
				return "ServerHostbannerGfxUrl";
			case PropertyIdType.ServerHostbannerGfxInterval:
				return "ServerHostbannerGfxInterval";
			case PropertyIdType.ServerPrioritySpeakerDimmModificator:
				return "ServerPrioritySpeakerDimmModificator";
			case PropertyIdType.ServerHostbuttonTooltip:
				return "ServerHostbuttonTooltip";
			case PropertyIdType.ServerHostbuttonUrl:
				return "ServerHostbuttonUrl";
			case PropertyIdType.ServerHostbuttonGfxUrl:
				return "ServerHostbuttonGfxUrl";
			case PropertyIdType.ServerPhoneticName:
				return "ServerPhoneticName";
			case PropertyIdType.ServerIconId:
				return "ServerIconId";
			case PropertyIdType.ServerIp:
				return "ServerIp";
			case PropertyIdType.ServerAskForPrivilegekey:
				return "ServerAskForPrivilegekey";
			case PropertyIdType.ServerHostbannerMode:
				return "ServerHostbannerMode";
			case PropertyIdType.ServerTempChannelDefaultDeleteDelay:
				return "ServerTempChannelDefaultDeleteDelay";
			case PropertyIdType.ServerProtocolVersion:
				return "ServerProtocolVersion";
			case PropertyIdType.ServerLicense:
				return "ServerLicense";
			case PropertyIdType.ConnectionOwnClient:
				return "ConnectionOwnClient";
			case PropertyIdType.ChatEntrySenderClient:
				return "ChatEntrySenderClient";
			case PropertyIdType.ChatEntryText:
				return "ChatEntryText";
			case PropertyIdType.ChatEntryDate:
				return "ChatEntryDate";
			case PropertyIdType.ChatEntryMode:
				return "ChatEntryMode";
			default:
				throw new Exception("Unknown property id type");
			}
		}

		public static ulong[] PropertyIdToIdPath(PropertyId id, ConnectionId conId)
		{
			switch (id.Type)
			{
			case PropertyIdType.ServerGroup: {
				ulong i0 = id.ServerGroup.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Groups, i0};
			}
			case PropertyIdType.File: {
				ulong i0 = id.File0.Value;
				ulong i1 = (ulong) id.File1.GetHashCode();
				return new ulong[] {conId.Value, i0, i1};
			}
			case PropertyIdType.OptionalChannelData: {
				ulong i0 = id.OptionalChannelData.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.OptionalData};
			}
			case PropertyIdType.Channel: {
				ulong i0 = id.Channel.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0};
			}
			case PropertyIdType.OptionalClientData: {
				ulong i0 = id.OptionalClientData.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.OptionalData};
			}
			case PropertyIdType.ConnectionClientData: {
				ulong i0 = id.ConnectionClientData.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData};
			}
			case PropertyIdType.Client: {
				ulong i0 = id.Client.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0};
			}
			case PropertyIdType.OptionalServerData: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData};
			}
			case PropertyIdType.ConnectionServerData: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData};
			}
			case PropertyIdType.Server: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server};
			}
			case PropertyIdType.Connection: {
				return new ulong[] {conId.Value};
			}
			case PropertyIdType.ChatEntry: {
				ulong i0 = id.ChatEntry.Value;
				return new ulong[] {conId.Value, i0};
			}

			case PropertyIdType.ServerGroupId: {
				ulong i0 = id.ServerGroupId.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Groups, i0, (ulong) ServerGroupPropertyId.Id};
			}
			case PropertyIdType.ServerGroupName: {
				ulong i0 = id.ServerGroupName.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Groups, i0, (ulong) ServerGroupPropertyId.Name};
			}
			case PropertyIdType.ServerGroupGroupType: {
				ulong i0 = id.ServerGroupGroupType.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Groups, i0, (ulong) ServerGroupPropertyId.GroupType};
			}
			case PropertyIdType.ServerGroupIconId: {
				ulong i0 = id.ServerGroupIconId.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Groups, i0, (ulong) ServerGroupPropertyId.IconId};
			}
			case PropertyIdType.ServerGroupIsPermanent: {
				ulong i0 = id.ServerGroupIsPermanent.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Groups, i0, (ulong) ServerGroupPropertyId.IsPermanent};
			}
			case PropertyIdType.ServerGroupSortId: {
				ulong i0 = id.ServerGroupSortId.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Groups, i0, (ulong) ServerGroupPropertyId.SortId};
			}
			case PropertyIdType.ServerGroupNamingMode: {
				ulong i0 = id.ServerGroupNamingMode.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Groups, i0, (ulong) ServerGroupPropertyId.NamingMode};
			}
			case PropertyIdType.ServerGroupNeededModifyPower: {
				ulong i0 = id.ServerGroupNeededModifyPower.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Groups, i0, (ulong) ServerGroupPropertyId.NeededModifyPower};
			}
			case PropertyIdType.ServerGroupNeededMemberAddPower: {
				ulong i0 = id.ServerGroupNeededMemberAddPower.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Groups, i0, (ulong) ServerGroupPropertyId.NeededMemberAddPower};
			}
			case PropertyIdType.ServerGroupNeededMemberRemovePower: {
				ulong i0 = id.ServerGroupNeededMemberRemovePower.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Groups, i0, (ulong) ServerGroupPropertyId.NeededMemberRemovePower};
			}
			case PropertyIdType.FilePath: {
				ulong i0 = id.FilePath0.Value;
				ulong i1 = (ulong) id.FilePath1.GetHashCode();
				return new ulong[] {conId.Value, i0, i1, (ulong) FilePropertyId.Path};
			}
			case PropertyIdType.FileSize: {
				ulong i0 = id.FileSize0.Value;
				ulong i1 = (ulong) id.FileSize1.GetHashCode();
				return new ulong[] {conId.Value, i0, i1, (ulong) FilePropertyId.Size};
			}
			case PropertyIdType.FileLastChanged: {
				ulong i0 = id.FileLastChanged0.Value;
				ulong i1 = (ulong) id.FileLastChanged1.GetHashCode();
				return new ulong[] {conId.Value, i0, i1, (ulong) FilePropertyId.LastChanged};
			}
			case PropertyIdType.FileIsFile: {
				ulong i0 = id.FileIsFile0.Value;
				ulong i1 = (ulong) id.FileIsFile1.GetHashCode();
				return new ulong[] {conId.Value, i0, i1, (ulong) FilePropertyId.IsFile};
			}
			case PropertyIdType.OptionalChannelDataDescription: {
				ulong i0 = id.OptionalChannelDataDescription.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.OptionalData, (ulong) OptionalChannelDataPropertyId.Description};
			}
			case PropertyIdType.ChannelId: {
				ulong i0 = id.ChannelId.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.Id};
			}
			case PropertyIdType.ChannelParent: {
				ulong i0 = id.ChannelParent.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.Parent};
			}
			case PropertyIdType.ChannelName: {
				ulong i0 = id.ChannelName.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.Name};
			}
			case PropertyIdType.ChannelTopic: {
				ulong i0 = id.ChannelTopic.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.Topic};
			}
			case PropertyIdType.ChannelCodec: {
				ulong i0 = id.ChannelCodec.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.Codec};
			}
			case PropertyIdType.ChannelCodecQuality: {
				ulong i0 = id.ChannelCodecQuality.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.CodecQuality};
			}
			case PropertyIdType.ChannelMaxClients: {
				ulong i0 = id.ChannelMaxClients.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.MaxClients};
			}
			case PropertyIdType.ChannelMaxFamilyClients: {
				ulong i0 = id.ChannelMaxFamilyClients.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.MaxFamilyClients};
			}
			case PropertyIdType.ChannelOrder: {
				ulong i0 = id.ChannelOrder.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.Order};
			}
			case PropertyIdType.ChannelChannelType: {
				ulong i0 = id.ChannelChannelType.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.ChannelType};
			}
			case PropertyIdType.ChannelIsDefault: {
				ulong i0 = id.ChannelIsDefault.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.IsDefault};
			}
			case PropertyIdType.ChannelHasPassword: {
				ulong i0 = id.ChannelHasPassword.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.HasPassword};
			}
			case PropertyIdType.ChannelCodecLatencyFactor: {
				ulong i0 = id.ChannelCodecLatencyFactor.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.CodecLatencyFactor};
			}
			case PropertyIdType.ChannelIsUnencrypted: {
				ulong i0 = id.ChannelIsUnencrypted.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.IsUnencrypted};
			}
			case PropertyIdType.ChannelDeleteDelay: {
				ulong i0 = id.ChannelDeleteDelay.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.DeleteDelay};
			}
			case PropertyIdType.ChannelNeededTalkPower: {
				ulong i0 = id.ChannelNeededTalkPower.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.NeededTalkPower};
			}
			case PropertyIdType.ChannelForcedSilence: {
				ulong i0 = id.ChannelForcedSilence.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.ForcedSilence};
			}
			case PropertyIdType.ChannelPhoneticName: {
				ulong i0 = id.ChannelPhoneticName.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.PhoneticName};
			}
			case PropertyIdType.ChannelIconId: {
				ulong i0 = id.ChannelIconId.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.IconId};
			}
			case PropertyIdType.ChannelIsPrivate: {
				ulong i0 = id.ChannelIsPrivate.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.IsPrivate};
			}
			case PropertyIdType.ChannelSubscribed: {
				ulong i0 = id.ChannelSubscribed.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Channels, i0, (ulong) ChannelPropertyId.Subscribed};
			}
			case PropertyIdType.OptionalClientDataVersion: {
				ulong i0 = id.OptionalClientDataVersion.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.OptionalData, (ulong) OptionalClientDataPropertyId.Version};
			}
			case PropertyIdType.OptionalClientDataPlatform: {
				ulong i0 = id.OptionalClientDataPlatform.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.OptionalData, (ulong) OptionalClientDataPropertyId.Platform};
			}
			case PropertyIdType.OptionalClientDataLoginName: {
				ulong i0 = id.OptionalClientDataLoginName.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.OptionalData, (ulong) OptionalClientDataPropertyId.LoginName};
			}
			case PropertyIdType.OptionalClientDataCreated: {
				ulong i0 = id.OptionalClientDataCreated.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.OptionalData, (ulong) OptionalClientDataPropertyId.Created};
			}
			case PropertyIdType.OptionalClientDataLastConnected: {
				ulong i0 = id.OptionalClientDataLastConnected.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.OptionalData, (ulong) OptionalClientDataPropertyId.LastConnected};
			}
			case PropertyIdType.OptionalClientDataTotalConnection: {
				ulong i0 = id.OptionalClientDataTotalConnection.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.OptionalData, (ulong) OptionalClientDataPropertyId.TotalConnection};
			}
			case PropertyIdType.OptionalClientDataMonthBytesUploaded: {
				ulong i0 = id.OptionalClientDataMonthBytesUploaded.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.OptionalData, (ulong) OptionalClientDataPropertyId.MonthBytesUploaded};
			}
			case PropertyIdType.OptionalClientDataMonthBytesDownloaded: {
				ulong i0 = id.OptionalClientDataMonthBytesDownloaded.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.OptionalData, (ulong) OptionalClientDataPropertyId.MonthBytesDownloaded};
			}
			case PropertyIdType.OptionalClientDataTotalBytesUploaded: {
				ulong i0 = id.OptionalClientDataTotalBytesUploaded.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.OptionalData, (ulong) OptionalClientDataPropertyId.TotalBytesUploaded};
			}
			case PropertyIdType.OptionalClientDataTotalBytesDownloaded: {
				ulong i0 = id.OptionalClientDataTotalBytesDownloaded.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.OptionalData, (ulong) OptionalClientDataPropertyId.TotalBytesDownloaded};
			}
			case PropertyIdType.ConnectionClientDataPing: {
				ulong i0 = id.ConnectionClientDataPing.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.Ping};
			}
			case PropertyIdType.ConnectionClientDataPingDeviation: {
				ulong i0 = id.ConnectionClientDataPingDeviation.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.PingDeviation};
			}
			case PropertyIdType.ConnectionClientDataConnectedTime: {
				ulong i0 = id.ConnectionClientDataConnectedTime.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.ConnectedTime};
			}
			case PropertyIdType.ConnectionClientDataClientAddress: {
				ulong i0 = id.ConnectionClientDataClientAddress.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.ClientAddress};
			}
			case PropertyIdType.ConnectionClientDataPacketsSentSpeech: {
				ulong i0 = id.ConnectionClientDataPacketsSentSpeech.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.PacketsSentSpeech};
			}
			case PropertyIdType.ConnectionClientDataPacketsSentKeepalive: {
				ulong i0 = id.ConnectionClientDataPacketsSentKeepalive.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.PacketsSentKeepalive};
			}
			case PropertyIdType.ConnectionClientDataPacketsSentControl: {
				ulong i0 = id.ConnectionClientDataPacketsSentControl.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.PacketsSentControl};
			}
			case PropertyIdType.ConnectionClientDataBytesSentSpeech: {
				ulong i0 = id.ConnectionClientDataBytesSentSpeech.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BytesSentSpeech};
			}
			case PropertyIdType.ConnectionClientDataBytesSentKeepalive: {
				ulong i0 = id.ConnectionClientDataBytesSentKeepalive.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BytesSentKeepalive};
			}
			case PropertyIdType.ConnectionClientDataBytesSentControl: {
				ulong i0 = id.ConnectionClientDataBytesSentControl.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BytesSentControl};
			}
			case PropertyIdType.ConnectionClientDataPacketsReceivedSpeech: {
				ulong i0 = id.ConnectionClientDataPacketsReceivedSpeech.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.PacketsReceivedSpeech};
			}
			case PropertyIdType.ConnectionClientDataPacketsReceivedKeepalive: {
				ulong i0 = id.ConnectionClientDataPacketsReceivedKeepalive.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.PacketsReceivedKeepalive};
			}
			case PropertyIdType.ConnectionClientDataPacketsReceivedControl: {
				ulong i0 = id.ConnectionClientDataPacketsReceivedControl.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.PacketsReceivedControl};
			}
			case PropertyIdType.ConnectionClientDataBytesReceivedSpeech: {
				ulong i0 = id.ConnectionClientDataBytesReceivedSpeech.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BytesReceivedSpeech};
			}
			case PropertyIdType.ConnectionClientDataBytesReceivedKeepalive: {
				ulong i0 = id.ConnectionClientDataBytesReceivedKeepalive.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BytesReceivedKeepalive};
			}
			case PropertyIdType.ConnectionClientDataBytesReceivedControl: {
				ulong i0 = id.ConnectionClientDataBytesReceivedControl.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BytesReceivedControl};
			}
			case PropertyIdType.ConnectionClientDataServerToClientPacketlossSpeech: {
				ulong i0 = id.ConnectionClientDataServerToClientPacketlossSpeech.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.ServerToClientPacketlossSpeech};
			}
			case PropertyIdType.ConnectionClientDataServerToClientPacketlossKeepalive: {
				ulong i0 = id.ConnectionClientDataServerToClientPacketlossKeepalive.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.ServerToClientPacketlossKeepalive};
			}
			case PropertyIdType.ConnectionClientDataServerToClientPacketlossControl: {
				ulong i0 = id.ConnectionClientDataServerToClientPacketlossControl.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.ServerToClientPacketlossControl};
			}
			case PropertyIdType.ConnectionClientDataServerToClientPacketlossTotal: {
				ulong i0 = id.ConnectionClientDataServerToClientPacketlossTotal.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.ServerToClientPacketlossTotal};
			}
			case PropertyIdType.ConnectionClientDataClientToServerPacketlossSpeech: {
				ulong i0 = id.ConnectionClientDataClientToServerPacketlossSpeech.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.ClientToServerPacketlossSpeech};
			}
			case PropertyIdType.ConnectionClientDataClientToServerPacketlossKeepalive: {
				ulong i0 = id.ConnectionClientDataClientToServerPacketlossKeepalive.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.ClientToServerPacketlossKeepalive};
			}
			case PropertyIdType.ConnectionClientDataClientToServerPacketlossControl: {
				ulong i0 = id.ConnectionClientDataClientToServerPacketlossControl.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.ClientToServerPacketlossControl};
			}
			case PropertyIdType.ConnectionClientDataClientToServerPacketlossTotal: {
				ulong i0 = id.ConnectionClientDataClientToServerPacketlossTotal.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.ClientToServerPacketlossTotal};
			}
			case PropertyIdType.ConnectionClientDataBandwidthSentLastSecondSpeech: {
				ulong i0 = id.ConnectionClientDataBandwidthSentLastSecondSpeech.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BandwidthSentLastSecondSpeech};
			}
			case PropertyIdType.ConnectionClientDataBandwidthSentLastSecondKeepalive: {
				ulong i0 = id.ConnectionClientDataBandwidthSentLastSecondKeepalive.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BandwidthSentLastSecondKeepalive};
			}
			case PropertyIdType.ConnectionClientDataBandwidthSentLastSecondControl: {
				ulong i0 = id.ConnectionClientDataBandwidthSentLastSecondControl.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BandwidthSentLastSecondControl};
			}
			case PropertyIdType.ConnectionClientDataBandwidthSentLastMinuteSpeech: {
				ulong i0 = id.ConnectionClientDataBandwidthSentLastMinuteSpeech.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BandwidthSentLastMinuteSpeech};
			}
			case PropertyIdType.ConnectionClientDataBandwidthSentLastMinuteKeepalive: {
				ulong i0 = id.ConnectionClientDataBandwidthSentLastMinuteKeepalive.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BandwidthSentLastMinuteKeepalive};
			}
			case PropertyIdType.ConnectionClientDataBandwidthSentLastMinuteControl: {
				ulong i0 = id.ConnectionClientDataBandwidthSentLastMinuteControl.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BandwidthSentLastMinuteControl};
			}
			case PropertyIdType.ConnectionClientDataBandwidthReceivedLastSecondSpeech: {
				ulong i0 = id.ConnectionClientDataBandwidthReceivedLastSecondSpeech.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BandwidthReceivedLastSecondSpeech};
			}
			case PropertyIdType.ConnectionClientDataBandwidthReceivedLastSecondKeepalive: {
				ulong i0 = id.ConnectionClientDataBandwidthReceivedLastSecondKeepalive.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BandwidthReceivedLastSecondKeepalive};
			}
			case PropertyIdType.ConnectionClientDataBandwidthReceivedLastSecondControl: {
				ulong i0 = id.ConnectionClientDataBandwidthReceivedLastSecondControl.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BandwidthReceivedLastSecondControl};
			}
			case PropertyIdType.ConnectionClientDataBandwidthReceivedLastMinuteSpeech: {
				ulong i0 = id.ConnectionClientDataBandwidthReceivedLastMinuteSpeech.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BandwidthReceivedLastMinuteSpeech};
			}
			case PropertyIdType.ConnectionClientDataBandwidthReceivedLastMinuteKeepalive: {
				ulong i0 = id.ConnectionClientDataBandwidthReceivedLastMinuteKeepalive.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BandwidthReceivedLastMinuteKeepalive};
			}
			case PropertyIdType.ConnectionClientDataBandwidthReceivedLastMinuteControl: {
				ulong i0 = id.ConnectionClientDataBandwidthReceivedLastMinuteControl.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.BandwidthReceivedLastMinuteControl};
			}
			case PropertyIdType.ConnectionClientDataFiletransferBandwidthSent: {
				ulong i0 = id.ConnectionClientDataFiletransferBandwidthSent.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.FiletransferBandwidthSent};
			}
			case PropertyIdType.ConnectionClientDataFiletransferBandwidthReceived: {
				ulong i0 = id.ConnectionClientDataFiletransferBandwidthReceived.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.FiletransferBandwidthReceived};
			}
			case PropertyIdType.ConnectionClientDataIdleTime: {
				ulong i0 = id.ConnectionClientDataIdleTime.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ConnectionData, (ulong) ConnectionClientDataPropertyId.IdleTime};
			}
			case PropertyIdType.ClientId: {
				ulong i0 = id.ClientId.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.Id};
			}
			case PropertyIdType.ClientChannel: {
				ulong i0 = id.ClientChannel.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.Channel};
			}
			case PropertyIdType.ClientUid: {
				ulong i0 = id.ClientUid.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.Uid};
			}
			case PropertyIdType.ClientName: {
				ulong i0 = id.ClientName.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.Name};
			}
			case PropertyIdType.ClientInputMuted: {
				ulong i0 = id.ClientInputMuted.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.InputMuted};
			}
			case PropertyIdType.ClientOutputMuted: {
				ulong i0 = id.ClientOutputMuted.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.OutputMuted};
			}
			case PropertyIdType.ClientOutputOnlyMuted: {
				ulong i0 = id.ClientOutputOnlyMuted.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.OutputOnlyMuted};
			}
			case PropertyIdType.ClientInputHardwareEnabled: {
				ulong i0 = id.ClientInputHardwareEnabled.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.InputHardwareEnabled};
			}
			case PropertyIdType.ClientOutputHardwareEnabled: {
				ulong i0 = id.ClientOutputHardwareEnabled.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.OutputHardwareEnabled};
			}
			case PropertyIdType.ClientTalkPowerGranted: {
				ulong i0 = id.ClientTalkPowerGranted.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.TalkPowerGranted};
			}
			case PropertyIdType.ClientMetadata: {
				ulong i0 = id.ClientMetadata.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.Metadata};
			}
			case PropertyIdType.ClientIsRecording: {
				ulong i0 = id.ClientIsRecording.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.IsRecording};
			}
			case PropertyIdType.ClientDatabaseId: {
				ulong i0 = id.ClientDatabaseId.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.DatabaseId};
			}
			case PropertyIdType.ClientChannelGroup: {
				ulong i0 = id.ClientChannelGroup.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ChannelGroup};
			}
			case PropertyIdType.ClientServerGroup: {
				ulong i0 = id.ClientServerGroup0.Value;
				ulong i1 = id.ClientServerGroup1.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ServerGroups, i1};
			}
			case PropertyIdType.ClientAwayMessage: {
				ulong i0 = id.ClientAwayMessage.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.AwayMessage};
			}
			case PropertyIdType.ClientClientType: {
				ulong i0 = id.ClientClientType.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.ClientType};
			}
			case PropertyIdType.ClientAvatarHash: {
				ulong i0 = id.ClientAvatarHash.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.AvatarHash};
			}
			case PropertyIdType.ClientTalkPower: {
				ulong i0 = id.ClientTalkPower.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.TalkPower};
			}
			case PropertyIdType.ClientTalkPowerRequest: {
				ulong i0 = id.ClientTalkPowerRequest.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.TalkPowerRequest};
			}
			case PropertyIdType.ClientDescription: {
				ulong i0 = id.ClientDescription.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.Description};
			}
			case PropertyIdType.ClientIsPrioritySpeaker: {
				ulong i0 = id.ClientIsPrioritySpeaker.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.IsPrioritySpeaker};
			}
			case PropertyIdType.ClientUnreadMessages: {
				ulong i0 = id.ClientUnreadMessages.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.UnreadMessages};
			}
			case PropertyIdType.ClientPhoneticName: {
				ulong i0 = id.ClientPhoneticName.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.PhoneticName};
			}
			case PropertyIdType.ClientNeededServerqueryViewPower: {
				ulong i0 = id.ClientNeededServerqueryViewPower.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.NeededServerqueryViewPower};
			}
			case PropertyIdType.ClientIconId: {
				ulong i0 = id.ClientIconId.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.IconId};
			}
			case PropertyIdType.ClientIsChannelCommander: {
				ulong i0 = id.ClientIsChannelCommander.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.IsChannelCommander};
			}
			case PropertyIdType.ClientCountryCode: {
				ulong i0 = id.ClientCountryCode.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.CountryCode};
			}
			case PropertyIdType.ClientInheritedChannelGroupFromChannel: {
				ulong i0 = id.ClientInheritedChannelGroupFromChannel.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.InheritedChannelGroupFromChannel};
			}
			case PropertyIdType.ClientBadges: {
				ulong i0 = id.ClientBadges.Value;
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Clients, i0, (ulong) ClientPropertyId.Badges};
			}
			case PropertyIdType.OptionalServerDataConnectionCount: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.ConnectionCount};
			}
			case PropertyIdType.OptionalServerDataChannelCount: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.ChannelCount};
			}
			case PropertyIdType.OptionalServerDataUptime: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.Uptime};
			}
			case PropertyIdType.OptionalServerDataHasPassword: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.HasPassword};
			}
			case PropertyIdType.OptionalServerDataDefaultChannelAdminGroup: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.DefaultChannelAdminGroup};
			}
			case PropertyIdType.OptionalServerDataMaxDownloadTotalBandwith: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.MaxDownloadTotalBandwith};
			}
			case PropertyIdType.OptionalServerDataMaxUploadTotalBandwith: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.MaxUploadTotalBandwith};
			}
			case PropertyIdType.OptionalServerDataComplainAutobanCount: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.ComplainAutobanCount};
			}
			case PropertyIdType.OptionalServerDataComplainAutobanTime: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.ComplainAutobanTime};
			}
			case PropertyIdType.OptionalServerDataComplainRemoveTime: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.ComplainRemoveTime};
			}
			case PropertyIdType.OptionalServerDataMinClientsForceSilence: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.MinClientsForceSilence};
			}
			case PropertyIdType.OptionalServerDataAntifloodPointsTickReduce: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.AntifloodPointsTickReduce};
			}
			case PropertyIdType.OptionalServerDataAntifloodPointsNeededCommandBlock: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.AntifloodPointsNeededCommandBlock};
			}
			case PropertyIdType.OptionalServerDataClientCount: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.ClientCount};
			}
			case PropertyIdType.OptionalServerDataQueryCount: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.QueryCount};
			}
			case PropertyIdType.OptionalServerDataQueryOnlineCount: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.QueryOnlineCount};
			}
			case PropertyIdType.OptionalServerDataDownloadQuota: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.DownloadQuota};
			}
			case PropertyIdType.OptionalServerDataUploadQuota: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.UploadQuota};
			}
			case PropertyIdType.OptionalServerDataMonthBytesDownloaded: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.MonthBytesDownloaded};
			}
			case PropertyIdType.OptionalServerDataMonthBytesUploaded: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.MonthBytesUploaded};
			}
			case PropertyIdType.OptionalServerDataTotalBytesDownloaded: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.TotalBytesDownloaded};
			}
			case PropertyIdType.OptionalServerDataTotalBytesUploaded: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.TotalBytesUploaded};
			}
			case PropertyIdType.OptionalServerDataPort: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.Port};
			}
			case PropertyIdType.OptionalServerDataAutostart: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.Autostart};
			}
			case PropertyIdType.OptionalServerDataMachineId: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.MachineId};
			}
			case PropertyIdType.OptionalServerDataNeededIdentitySecurityLevel: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.NeededIdentitySecurityLevel};
			}
			case PropertyIdType.OptionalServerDataLogClient: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.LogClient};
			}
			case PropertyIdType.OptionalServerDataLogQuery: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.LogQuery};
			}
			case PropertyIdType.OptionalServerDataLogChannel: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.LogChannel};
			}
			case PropertyIdType.OptionalServerDataLogPermissions: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.LogPermissions};
			}
			case PropertyIdType.OptionalServerDataLogServer: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.LogServer};
			}
			case PropertyIdType.OptionalServerDataLogFileTransfer: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.LogFileTransfer};
			}
			case PropertyIdType.OptionalServerDataMinClientVersion: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.MinClientVersion};
			}
			case PropertyIdType.OptionalServerDataReservedSlots: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.ReservedSlots};
			}
			case PropertyIdType.OptionalServerDataTotalPacketlossSpeech: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.TotalPacketlossSpeech};
			}
			case PropertyIdType.OptionalServerDataTotalPacketlossKeepalive: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.TotalPacketlossKeepalive};
			}
			case PropertyIdType.OptionalServerDataTotalPacketlossControl: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.TotalPacketlossControl};
			}
			case PropertyIdType.OptionalServerDataTotalPacketlossTotal: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.TotalPacketlossTotal};
			}
			case PropertyIdType.OptionalServerDataTotalPing: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.TotalPing};
			}
			case PropertyIdType.OptionalServerDataWeblistEnabled: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.WeblistEnabled};
			}
			case PropertyIdType.OptionalServerDataMinAndroidVersion: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.MinAndroidVersion};
			}
			case PropertyIdType.OptionalServerDataMinIosVersion: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.OptionalData, (ulong) OptionalServerDataPropertyId.MinIosVersion};
			}
			case PropertyIdType.ConnectionServerDataFileTransferBandwidthSent: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.FileTransferBandwidthSent};
			}
			case PropertyIdType.ConnectionServerDataFileTransferBandwidthReceived: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.FileTransferBandwidthReceived};
			}
			case PropertyIdType.ConnectionServerDataFileTransferBytesSentTotal: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.FileTransferBytesSentTotal};
			}
			case PropertyIdType.ConnectionServerDataFileTransferBytesReceivedTotal: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.FileTransferBytesReceivedTotal};
			}
			case PropertyIdType.ConnectionServerDataPacketsSentTotal: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.PacketsSentTotal};
			}
			case PropertyIdType.ConnectionServerDataBytesSentTotal: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.BytesSentTotal};
			}
			case PropertyIdType.ConnectionServerDataPacketsReceivedTotal: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.PacketsReceivedTotal};
			}
			case PropertyIdType.ConnectionServerDataBytesReceivedTotal: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.BytesReceivedTotal};
			}
			case PropertyIdType.ConnectionServerDataBandwidthSentLastSecondTotal: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.BandwidthSentLastSecondTotal};
			}
			case PropertyIdType.ConnectionServerDataBandwidthSentLastMinuteTotal: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.BandwidthSentLastMinuteTotal};
			}
			case PropertyIdType.ConnectionServerDataBandwidthReceivedLastSecondTotal: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.BandwidthReceivedLastSecondTotal};
			}
			case PropertyIdType.ConnectionServerDataBandwidthReceivedLastMinuteTotal: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.BandwidthReceivedLastMinuteTotal};
			}
			case PropertyIdType.ConnectionServerDataConnectedTime: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.ConnectedTime};
			}
			case PropertyIdType.ConnectionServerDataPacketlossTotal: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.PacketlossTotal};
			}
			case PropertyIdType.ConnectionServerDataPing: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ConnectionData, (ulong) ConnectionServerDataPropertyId.Ping};
			}
			case PropertyIdType.ServerUid: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Uid};
			}
			case PropertyIdType.ServerVirtualServerId: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.VirtualServerId};
			}
			case PropertyIdType.ServerName: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Name};
			}
			case PropertyIdType.ServerWelcomeMessage: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.WelcomeMessage};
			}
			case PropertyIdType.ServerPlatform: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Platform};
			}
			case PropertyIdType.ServerVersion: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Version};
			}
			case PropertyIdType.ServerMaxClients: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.MaxClients};
			}
			case PropertyIdType.ServerCreated: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Created};
			}
			case PropertyIdType.ServerCodecEncryptionMode: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.CodecEncryptionMode};
			}
			case PropertyIdType.ServerHostmessage: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Hostmessage};
			}
			case PropertyIdType.ServerHostmessageMode: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.HostmessageMode};
			}
			case PropertyIdType.ServerDefaultServerGroup: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.DefaultServerGroup};
			}
			case PropertyIdType.ServerDefaultChannelGroup: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.DefaultChannelGroup};
			}
			case PropertyIdType.ServerHostbannerUrl: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.HostbannerUrl};
			}
			case PropertyIdType.ServerHostbannerGfxUrl: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.HostbannerGfxUrl};
			}
			case PropertyIdType.ServerHostbannerGfxInterval: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.HostbannerGfxInterval};
			}
			case PropertyIdType.ServerPrioritySpeakerDimmModificator: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.PrioritySpeakerDimmModificator};
			}
			case PropertyIdType.ServerHostbuttonTooltip: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.HostbuttonTooltip};
			}
			case PropertyIdType.ServerHostbuttonUrl: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.HostbuttonUrl};
			}
			case PropertyIdType.ServerHostbuttonGfxUrl: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.HostbuttonGfxUrl};
			}
			case PropertyIdType.ServerPhoneticName: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.PhoneticName};
			}
			case PropertyIdType.ServerIconId: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.IconId};
			}
			case PropertyIdType.ServerIp: {
				ulong i0 = (ulong) id.ServerIp.GetHashCode();
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.Ips, i0};
			}
			case PropertyIdType.ServerAskForPrivilegekey: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.AskForPrivilegekey};
			}
			case PropertyIdType.ServerHostbannerMode: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.HostbannerMode};
			}
			case PropertyIdType.ServerTempChannelDefaultDeleteDelay: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.TempChannelDefaultDeleteDelay};
			}
			case PropertyIdType.ServerProtocolVersion: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.ProtocolVersion};
			}
			case PropertyIdType.ServerLicense: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.Server, (ulong) ServerPropertyId.License};
			}
			case PropertyIdType.ConnectionOwnClient: {
				return new ulong[] {conId.Value, (ulong) ConnectionPropertyId.OwnClient};
			}
			case PropertyIdType.ChatEntrySenderClient: {
				ulong i0 = id.ChatEntrySenderClient.Value;
				return new ulong[] {conId.Value, i0, (ulong) ChatEntryPropertyId.SenderClient};
			}
			case PropertyIdType.ChatEntryText: {
				ulong i0 = id.ChatEntryText.Value;
				return new ulong[] {conId.Value, i0, (ulong) ChatEntryPropertyId.Text};
			}
			case PropertyIdType.ChatEntryDate: {
				ulong i0 = id.ChatEntryDate.Value;
				return new ulong[] {conId.Value, i0, (ulong) ChatEntryPropertyId.Date};
			}
			case PropertyIdType.ChatEntryMode: {
				ulong i0 = id.ChatEntryMode.Value;
				return new ulong[] {conId.Value, i0, (ulong) ChatEntryPropertyId.Mode};
			}
			default:
				throw new Exception("Unknown property id type");
			}
		}
	}
}
