using DasBoot.Api.Api;
using DasBoot.Api.Pilot;
using DasBoot.Api.Serialization;
using DasBoot.Api.Stm32;

var builder = WebApplication.CreateSlimBuilder(args);

builder.Logging.ClearProviders();
builder.Logging.AddSimpleConsole(options =>
{
    options.SingleLine = true;
    options.TimestampFormat = "yyyy-MM-dd HH:mm:ss ";
});

builder.Services.ConfigureHttpJsonOptions(options =>
{
    options.SerializerOptions.TypeInfoResolverChain.Insert(0, ApiJsonContext.Default);
});

var stm32Options = Stm32LinkOptions.FromConfiguration(builder.Configuration);
var pilotOptions = PilotLeaseOptions.FromConfiguration(builder.Configuration);

builder.Services.AddSingleton(stm32Options);
builder.Services.AddSingleton(pilotOptions);
builder.Services.AddSingleton<PilotSessionService>();
builder.Services.AddSingleton<TelemetryStore>();
builder.Services.AddSingleton<Stm32LinkService>();
builder.Services.AddSingleton<IStm32Link>(static services =>
    services.GetRequiredService<Stm32LinkService>());
builder.Services.AddHostedService(static services =>
    services.GetRequiredService<Stm32LinkService>());

var app = builder.Build();

app.UseDefaultFiles();
app.UseStaticFiles();
app.MapDasBootEndpoints();

app.Run();
