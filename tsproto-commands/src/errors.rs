use ::std::fmt;

#[repr(u32)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Error {
	/// General
	Ok                                          = 0x0000,
	Undefined                                   = 0x0001,
	NotImplemented                              = 0x0002,
	OkNoUpdate                                  = 0x0003,
	DontNotify                                  = 0x0004,
	LibTimeLimitReached                         = 0x0005,

	/// Dunno
	CommandNotFount                             = 0x0100,
	UnableToBindNetworkPort                     = 0x0101,
	NoNetworkPortAvailable                      = 0x0102,
	PortAlreadyInUse                            = 0x103,

	/// Client
	ClientInvalidId                             = 0x0200,
	ClientNicknameInuse                         = 0x0201,
	ClientProtocolLimitReached                  = 0x0203,
	ClientInvalidType                           = 0x0204,
	ClientAlreadySubscribed                     = 0x0205,
	ClientNotLoggedIn                           = 0x0206,
	ClientCouldNotValidateIdentity              = 0x0207,
	ClientInvalidPassword                       = 0x0208,
	ClientTooManyClonesConnected                = 0x0209,
	ClientVersionOutdated                       = 0x020a,
	ClientIsOnline                              = 0x020b,
	ClientIsFlooding                            = 0x020c,
	ClientHacked                                = 0x020d,
	ClientCannotVerifyNow                       = 0x020e,
	ClientLoginNotPermitted                     = 0x020f,
	ClientNotSubscripted                        = 0x0210,

	/// Channel
	ChannelInvalidId                            = 0x0300,
	ChannelProtocolLimitReached                 = 0x0301,
	ChannelAlreadyIn                            = 0x0302,
	ChannelnameInuse                            = 0x0303,
	ChannelNotEmpty                             = 0x0304,
	ChannelCannotDeleteDefault                  = 0x0305,
	ChannelDefaultRequirePermanent              = 0x0306,
	ChannelInvalidFlags                         = 0x0307,
	ChannelParentNotPermanent                   = 0x0308,
	ChannelMaxclientsReached                    = 0x0309,
	ChannelMaxfamilyReached                     = 0x030a,
	ChannelInvalidOrder                         = 0x030b,
	ChannelNoFiletransferSupported              = 0x030c,
	ChannelInvalidPassword                      = 0x030d,
	ChannelIsPrivateChannel                     = 0x030e,
	ChannelInvalidSecurityHash                  = 0x030f,

	/// Server
	ServerInvalidId                             = 0x0400,
	ServerRunning                               = 0x0401,
	ServerIsShuttingDown                        = 0x0402,
	ServerMaxclientsReached                     = 0x0403,
	ServerInvalidPassword                       = 0x0404,
	ServerDeploymentActive                      = 0x0405,
	ServerUnableToStopOwnServer                 = 0x0406,
	ServerIsVirtual                             = 0x0407,
	ServerWrongMachineid                        = 0x0408,
	ServerIsNotRunning                          = 0x0409,
	ServerIsBooting                             = 0x040a,
	ServerStatusInvalid                         = 0x040b,
	ServerModalQuit                             = 0x040c,
	ServerVersionOutdated                       = 0x040d,
	ServerDuplicatedRunning                     = 0x040e,

	/// Database
	Database                                    = 0x0500,
	DatabaseEmptyResult                         = 0x0501,
	DatabaseDuplicateEntry                      = 0x0502,
	DatabaseNoModifications                     = 0x0503,
	DatabaseConstraint                          = 0x0504,
	DatabaseReinvoke                            = 0x0505,

	/// Parameter
	ParameterQuote                              = 0x0600,
	ParameterInvalidCount                       = 0x0601,
	ParameterInvalid                            = 0x0602,
	ParameterNotFount                           = 0x0603,
	ParameterConvert                            = 0x0604,
	ParameterInvalidSize                        = 0x0605,
	ParameterMissing                            = 0x0606,
	ParameterChecksum                           = 0x0607,

	/// Unsorted, needs further investigation
	VsCritical                                  = 0x0700,
	ConnectionLost                              = 0x0701,
	NotConnected                                = 0x0702,
	NoCachedConnectionInfo                      = 0x0703,
	CurrentlyNotPossible                        = 0x0704,
	FailedConnectionInitialisation              = 0x0705,
	CouldNotResolveHostname                     = 0x0706,
	InvalidServerConnectionoHandlerId           = 0x0707,
	CouldNotInitialiseInputManager              = 0x0708,
	ClientlibraryNotInitialised                 = 0x0709,
	ServerlibraryNotInitialised                 = 0x070a,
	WhisperTooManyTargets                       = 0x070b,
	WhisperNoTargets                            = 0x070c,
	ConnectionIpProtocolMissing                 = 0x070d,

	/// File transfer
	FileInvalidName                             = 0x0800,
	FileInvalidPermissions                      = 0x0801,
	FileAlreadyExists                           = 0x0802,
	FileNotFound                                = 0x0803,
	FileIoError                                 = 0x0804,
	FileInvalidTransferId                       = 0x0805,
	FileInvalidPath                             = 0x0806,
	FileNoFilesAvailable                        = 0x0807,
	FileOverwriteExcludesResume                 = 0x0808,
	FileInvalidSize                             = 0x0809,
	FileAlreadyInUse                            = 0x080a,
	FileCouldNotOpenConnection                  = 0x080b,
	FileNoSpaceLeftOnDevice                     = 0x080c,
	FileExceedsFileSystemMaximumSize            = 0x080d,
	FileTransferConnectionTimeout               = 0x080e,
	FileConnectionLost                          = 0x080f,
	FileExceedsSuppliedSize                     = 0x0810,
	FileTransferComplete                        = 0x0811,
	FileTransferCanceled                        = 0x0812,
	FileTransferInterrupted                     = 0x0813,
	FileTransferServerQuotaExceeded             = 0x0814,
	FileTransferClientQuotaExceeded             = 0x0815,
	FileTransferReset                           = 0x0816,
	FileTransferLimitReached                    = 0x0817,


	/// Sound
	SoundPreprocessorDisabled                   = 0x09_00,
	SoundInternalPreprocessor                   = 0x09_01,
	SoundInternalEncoder                        = 0x09_02,
	SoundInternalPlayback                       = 0x09_03,
	SoundNoCaptureDeviceAvailable               = 0x09_04,
	SoundNoPlaybackDeviceAvailable              = 0x09_05,
	SoundCouldNotOpenCaptureDevice              = 0x09_06,
	SoundCouldNotOpenPlaybackDevice             = 0x09_07,
	SoundHandlerHasDevice                       = 0x09_08,
	SoundInvalidCaptureDevice                   = 0x09_09,
	SoundInvalidPlaybackDevice                  = 0x09_0a,
	SoundInvalidWave                            = 0x09_0b,
	SoundUnsupportedWave                        = 0x09_0c,
	SoundOpenWave                               = 0x09_0d,
	SoundInternalCapture                        = 0x09_0e,
	SoundDeviceInUse                            = 0x09_0f,
	SoundDeviceAlreadyRegisterred               = 0x09_10,
	SoundUnknownDevice                          = 0x09_11,
	SoundUnsupportedFrequency                   = 0x09_12,
	SoundInvalidChannelCount                    = 0x09_13,
	SoundReadWave                               = 0x09_14,
	/// For internal purposes only
	SoundNeedMoreData                           = 0x09_15,
	/// For internal purposes only
	SoundDeviceBusy                             = 0x09_16,
	SoundNoData                                 = 0x09_17,
	SoundChannelMaskMismatch                    = 0x09_18,

	/// Permissions
	PermissionsInvalidGroupId                   = 0x0a_00,
	PermissionsDuplicateEntry                   = 0x0a_01,
	PermissionsInvalidPermId                    = 0x0a_02,
	PermissionsEmptyResult                      = 0x0a_03,
	PermissionsDefaultGroupForbidden            = 0x0a_04,
	PermissionsInvalidSize                      = 0x0a_05,
	PermissionsInvalidValue                     = 0x0a_06,
	PermissionsGroupNotEmpty                    = 0x0a_07,
	PermissionsClientInsufficient               = 0x0a_08,
	PermissionsInsufficientGroupPower           = 0x0a_09,
	PermissionsInsufficientPermissionPower      = 0x0a_0a,
	PermissionsTemplateGroupIsUsed              = 0x0a_0b,
	Permissions                                 = 0x0a_0c,

	/// Accounting
	AccountingVirtualserverLimitReached         = 0x0b_00,
	AccountingSlotLimitReached                  = 0x0b_01,
	AccountingLicenseFileNotFound               = 0x0b_02,
	AccountingLicenseDateNotOk                  = 0x0b_03,
	AccountingUnableToConnectToServer           = 0x0b_04,
	AccountingUnknownError                      = 0x0b_05,
	AcountingServerError                        = 0x0b_06,
	AccountingInstanceLimitReached              = 0x0b_07,
	AccountingInstanceCheckError                = 0x0b_08,
	AccountingLicenseFileInvalid                = 0x0b_09,
	AccountingRunningElsewhere                  = 0x0b_0a,
	AccountingInstanceDuplicated                = 0x0b_0b,
	AccountingAlreadyStarted                    = 0x0b_0c,
	AccountingNotStarted                        = 0x0b_0d,
	AccountingToManyStarts                      = 0x0b_0e,

	/// Messages
	MessageInvalidId                            = 0x0c_00,

	/// Ban
	BanInvalidId                                = 0x0d_00,
	ConnectFailedBanned                         = 0x0d_01,
	RenameFailedBanned                          = 0x0d_02,
	BanFlooding                                 = 0x0d_03,

	/// Text to speech
	TtsUnableToInitialize                       = 0x0e_00,

	/// Privilege key
	PrivilegeKeyInvalid                         = 0x0f_00,

	/// Voip
	VoipPjsua                                   = 0x10_00,
	VoipAlreadyInitialized                      = 0x10_01,
	VoipTooManyAccounts                         = 0x10_02,
	VoipInvalidAccount                          = 0x10_03,
	VoipInternalError                           = 0x10_04,
	VoipInvalidConnectionId                     = 0x10_05,
	VoipCannotAnswerInitiatedCall               = 0x10_06,
	VoipNotInitialized                          = 0x10_07,

	/// Provisioning server
	ProvisioningInvalidPassword                 = 0x11_00,
	ProvisioningInvalidRequest                  = 0x11_01,
	ProvisioningNoSlotsAvailable                = 0x11_02,
	ProvisioningPoolMissing                     = 0x11_03,
	ProvisioningPoolUnkown                      = 0x11_04,
	ProvisioningUnknownIpLocation               = 0x11_05,
	ProvisioningInternalTriedExceeded           = 0x11_06,
	ProvisioningTooManySlotsRequested           = 0x11_07,
	ProvisioningTooManyReserved                 = 0x11_08,
	ProvisioningCouldNotConnect                 = 0x11_09,
	ProvisioningAuthServerNotConnected          = 0x11_10,
	ProvisioningAuthDataTooLarge                = 0x11_11,
	ProvisioningAlreadyInitialized              = 0x11_12,
	ProvisioningNotInitialized                  = 0x11_13,
	ProvisioningConnecting                      = 0x11_14,
	ProvisioningAlreadyConnected                = 0x11_15,
	ProvisioningNotConnected                    = 0x11_16,
	ProvisioningIoError                         = 0x11_17,
	ProvisioningInvalidTimeout                  = 0x11_18,
	ProvisioningTs3severNotFound                = 0x11_19,
	ProvisioningNoPermission                    = 0x11_1A,
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		fmt::Debug::fmt(self, f)
	}
}

impl ::std::error::Error for Error {
	fn description(&self) -> &str {
		"TeamSpeak error"
	}
}
